--[[--
CloudReader plugin for KOReader.

Sync reading progress and browse remote ebook-rs library.
Creates local placeholders that are downloaded on demand.

@module koplugin.cloudreader
--]]

local DataStorage = require("datastorage")
local Event = require("ui/event")
local InfoMessage = require("ui/widget/infomessage")
local InputDialog = require("ui/widget/inputdialog")
local JSON = require("json")
local LuaSettings = require("luasettings")
local MultiInputDialog = require("ui/widget/multiinputdialog")
local NetworkMgr = require("ui/network/manager")
local UIManager = require("ui/uimanager")
local WidgetContainer = require("ui/widget/container/widgetcontainer")
local http = require("socket.http")
local lfs = require("libs/libkoreader-lfs")
local logger = require("logger")
local ltn12 = require("ltn12")
local socket = require("socket")
local socketutil = require("socketutil")
local _ = require("gettext")
local T = require("ffi/util").template

local CloudReader = WidgetContainer:extend{
    name = "cloudreader",
    is_doc_only = false,
}

local TIMEOUT_BLOCK = 5
local TIMEOUT_TOTAL = 15
local TIMEOUT_DOWNLOAD_BLOCK = 15
local TIMEOUT_DOWNLOAD_TOTAL = 300
local PLACEHOLDER_MAX_SIZE = 500 * 1024
local PLACEHOLDER_WIDTH = 600
local PLACEHOLDER_QUALITY = 90
local SYNC_INTERVAL = 30 * 60

function CloudReader:init()
    self:loadSettings()
    self:ensureLibraryDir()
    self.ui.menu:registerToMainMenu(self)
    self:patchReaderUI()
    
    if self.auto_sync and self:isLoggedIn() then
        UIManager:scheduleIn(5, function()
            self:autoSyncLibrary()
        end)
    end
end

function CloudReader:scheduleNextSync()
    if self.auto_sync and self:isLoggedIn() then
        UIManager:scheduleIn(SYNC_INTERVAL, function()
            self:autoSyncLibrary()
        end)
    end
end

function CloudReader:autoSyncLibrary()
    if not NetworkMgr:isConnected() then
        logger.dbg("CloudReader: No network, skipping auto-sync")
        self:scheduleNextSync()
        return
    end
    
    logger.info("CloudReader: Auto-sync starting...")
    UIManager:preventStandby()
    
    UIManager:show(InfoMessage:new{
        text = _("Syncing library..."),
        timeout = 1,
    })
    UIManager:forceRePaint()
    
    local base_url = self:getBaseUrl()
    local api_url = base_url .. "/api/library"
    
    socketutil:set_timeout(3, 8)
    local response_body = {}
    local code = socket.skip(1, http.request{
        url = api_url,
        method = "GET",
        sink = ltn12.sink.table(response_body),
    })
    socketutil:reset_timeout()
    
    if code ~= 200 then
        logger.dbg("CloudReader: Auto-sync failed (server unavailable)")
        UIManager:allowStandby()
        self:scheduleNextSync()
        return
    end
    
    local body = table.concat(response_body)
    local ok, result = pcall(JSON.decode, body)
    if not ok or not result or not result.books then
        logger.dbg("CloudReader: Auto-sync failed (invalid response)")
        UIManager:allowStandby()
        self:scheduleNextSync()
        return
    end
    
    local created = 0
    local total = #result.books
    for i, book in ipairs(result.books) do
        self.book_index[book.id] = book
        
        local rel_path = book.path
        local full_path = self.library_dir .. "/" .. rel_path
        local attr = lfs.attributes(full_path)
        
        if not attr or attr.size == 0 then
            local success, was_created = self:createPlaceholder(book)
            if success and was_created then
                created = created + 1
            end
        end
        
        if i % 50 == 0 then
            UIManager:show(InfoMessage:new{
                text = T(_("Syncing library... %1/%2"), i, total),
                timeout = 0.5,
            })
            UIManager:forceRePaint()
        end
    end
    
    if created > 0 then
        self:saveSettings()
        logger.info("CloudReader: Auto-sync placeholders done, created", created)
        UIManager:show(InfoMessage:new{
            text = T(_("Auto-sync done!\nNew: %1"), created),
            timeout = 2,
        })
    end
    
    self:syncAllSdr()
    UIManager:allowStandby()
    self:scheduleNextSync()
end

-- SDR Sync

function CloudReader:syncAllSdr()
    logger.info("CloudReader: Syncing SDR folders...")
    
    local base_url = self:getBaseUrl()
    socketutil:set_timeout(3, 8)
    local response_body = {}
    local code = socket.skip(1, http.request{
        url = base_url .. "/api/sync/sdr",
        method = "GET",
        headers = { ["Authorization"] = "Bearer " .. self.token },
        sink = ltn12.sink.table(response_body),
    })
    socketutil:reset_timeout()
    
    if code ~= 200 then
        logger.dbg("CloudReader: Failed to get SDR list from server")
        return
    end
    
    local body = table.concat(response_body)
    local ok, result = pcall(JSON.decode, body)
    if not ok or not result then
        logger.dbg("CloudReader: Invalid SDR list response")
        return
    end
    
    local server_sdrs = {}
    if result.sdrs then
        for _, sdr in ipairs(result.sdrs) do
            server_sdrs[sdr.book_id] = sdr
        end
    end
    
    local local_sdrs = self:scanLocalSdr()
    local uploaded = 0
    local downloaded = 0
    
    for book_id, local_info in pairs(local_sdrs) do
        local server_sdr = server_sdrs[book_id]
        
        if not server_sdr then
            if self:uploadSdr(book_id, local_info.sdr_path) then
                uploaded = uploaded + 1
            end
        elseif local_info.updated_at > server_sdr.updated_at then
            if self:uploadSdr(book_id, local_info.sdr_path) then
                uploaded = uploaded + 1
            end
        elseif server_sdr.updated_at > local_info.updated_at then
            if self:downloadAndMergeSdr(book_id, local_info.sdr_path, server_sdr) then
                downloaded = downloaded + 1
            end
        end
    end
    
    for book_id, server_sdr in pairs(server_sdrs) do
        if not local_sdrs[book_id] then
            local book = self.book_index[book_id]
            if book then
                local book_path = self.library_dir .. "/" .. book.path
                local sdr_path = book_path:gsub("%.[^%.]+$", ".sdr")
                if self:downloadSdr(book_id, sdr_path) then
                    downloaded = downloaded + 1
                end
            end
        end
    end
    
    if uploaded > 0 or downloaded > 0 then
        logger.info("CloudReader: SDR sync done, uploaded:", uploaded, "downloaded:", downloaded)
    else
        logger.dbg("CloudReader: SDR sync done, no changes")
    end
end

function CloudReader:scanLocalSdr()
    local sdrs = {}
    
    for book_id, book in pairs(self.book_index) do
        local book_path = self.library_dir .. "/" .. book.path
        local sdr_path = book_path:gsub("%.[^%.]+$", ".sdr")
        
        local attr = lfs.attributes(sdr_path)
        if attr and attr.mode == "directory" then
            local meta_file = nil
            local meta_time = 0
            
            for file in lfs.dir(sdr_path) do
                if file:match("^metadata%..*%.lua$") and not file:match("%.old$") then
                    local file_path = sdr_path .. "/" .. file
                    local file_attr = lfs.attributes(file_path)
                    if file_attr then
                        meta_file = file_path
                        meta_time = file_attr.modification
                    end
                end
            end
            
            if meta_file then
                sdrs[book_id] = {
                    sdr_path = sdr_path,
                    meta_file = meta_file,
                    updated_at = meta_time,
                }
            end
        end
    end
    
    return sdrs
end

function CloudReader:uploadSdr(book_id, sdr_path)
    logger.dbg("CloudReader: Uploading SDR for", book_id)
    
    local tmp_file = "/tmp/cloudreader_sdr_" .. book_id .. ".tar.gz"
    local parent_dir = sdr_path:match("(.+)/[^/]+$")
    local sdr_name = sdr_path:match("[^/]+$")
    
    local cmd = string.format('tar -czf "%s" -C "%s" "%s"', tmp_file, parent_dir, sdr_name)
    local ret = os.execute(cmd)
    
    if ret ~= 0 then
        logger.err("CloudReader: Failed to create tar.gz for SDR")
        return false
    end
    
    local file = io.open(tmp_file, "rb")
    if not file then
        logger.err("CloudReader: Failed to open tar.gz")
        return false
    end
    local data = file:read("*all")
    file:close()
    os.remove(tmp_file)
    
    local base_url = self:getBaseUrl()
    socketutil:set_timeout(5, 30)
    local code = socket.skip(1, http.request{
        url = base_url .. "/api/sync/sdr/" .. book_id,
        method = "PUT",
        headers = {
            ["Authorization"] = "Bearer " .. self.token,
            ["Content-Type"] = "application/gzip",
            ["Content-Length"] = #data,
        },
        source = ltn12.source.string(data),
    })
    socketutil:reset_timeout()
    
    if code == 200 then
        logger.info("CloudReader: Uploaded SDR for", book_id)
        return true
    else
        logger.err("CloudReader: Failed to upload SDR, code:", code)
        return false
    end
end

function CloudReader:downloadSdr(book_id, sdr_path)
    logger.dbg("CloudReader: Downloading SDR for", book_id)
    
    local tmp_file = "/tmp/cloudreader_sdr_" .. book_id .. ".tar.gz"
    
    local base_url = self:getBaseUrl()
    local file = io.open(tmp_file, "wb")
    if not file then
        logger.err("CloudReader: Failed to create temp file")
        return false
    end
    
    socketutil:set_timeout(5, 30)
    local code = socket.skip(1, http.request{
        url = base_url .. "/api/sync/sdr/" .. book_id,
        method = "GET",
        headers = { ["Authorization"] = "Bearer " .. self.token },
        sink = ltn12.sink.file(file),
    })
    socketutil:reset_timeout()
    
    if code ~= 200 then
        os.remove(tmp_file)
        logger.err("CloudReader: Failed to download SDR, code:", code)
        return false
    end
    
    local parent_dir = sdr_path:match("(.+)/[^/]+$")
    self:mkdirp(parent_dir)
    
    local cmd = string.format('tar -xzf "%s" -C "%s"', tmp_file, parent_dir)
    local ret = os.execute(cmd)
    os.remove(tmp_file)
    
    if ret ~= 0 then
        logger.err("CloudReader: Failed to extract SDR")
        return false
    end
    
    logger.info("CloudReader: Downloaded SDR for", book_id)
    return true
end

function CloudReader:downloadAndMergeSdr(book_id, local_sdr_path, server_info)
    local local_last_page = 0
    local local_percent = 0
    
    for file in lfs.dir(local_sdr_path) do
        if file:match("^metadata%..*%.lua$") and not file:match("%.old$") then
            local file_path = local_sdr_path .. "/" .. file
            local f = io.open(file_path, "r")
            if f then
                local content = f:read("*all")
                f:close()
                
                local page = content:match('%["last_page"%]%s*=%s*(%d+)')
                local percent = content:match('%["percent_finished"%]%s*=%s*([%d%.]+)')
                
                if page then local_last_page = tonumber(page) end
                if percent then local_percent = tonumber(percent) end
            end
        end
    end
    
    local server_last_page = server_info.last_page or 0
    local server_percent = server_info.percent_finished or 0
    
    if server_last_page > local_last_page or server_percent > local_percent then
        return self:downloadSdr(book_id, local_sdr_path)
    else
        return self:uploadSdr(book_id, local_sdr_path)
    end
end

-- Settings

function CloudReader:loadSettings()
    local settings_file = DataStorage:getSettingsDir() .. "/cloudreader.lua"
    self.settings = LuaSettings:open(settings_file)
    
    self.server_url = self.settings:readSetting("server_url") or ""
    self.username = self.settings:readSetting("username") or ""
    self.token = self.settings:readSetting("token") or ""
    self.user_id = self.settings:readSetting("user_id") or ""
    self.auto_sync = self.settings:readSetting("auto_sync")
    if self.auto_sync == nil then self.auto_sync = true end
    
    local base_dir = G_reader_settings:readSetting("home_dir")
    if not base_dir or base_dir == "" or base_dir == "." then
        base_dir = G_reader_settings:readSetting("lastdir")
    end
    if not base_dir or base_dir == "" or base_dir == "." then
        base_dir = DataStorage:getSettingsDir():match("(.+)/settings$") or DataStorage:getSettingsDir()
    end
    if not base_dir or base_dir == "" or base_dir == "." then
        if lfs.attributes("/mnt/ext1", "mode") == "directory" then
            base_dir = "/mnt/ext1"
        elseif lfs.attributes("/mnt/onboard", "mode") == "directory" then
            base_dir = "/mnt/onboard"
        else
            base_dir = "/tmp"
        end
    end
    
    local saved_dir = self.settings:readSetting("library_dir")
    if saved_dir and saved_dir:sub(1, 1) == "/" then
        self.library_dir = saved_dir
    else
        self.library_dir = base_dir .. "/cloudreader"
    end
    
    self.book_index = self.settings:readSetting("book_index") or {}
    logger.info("CloudReader: base_dir =", base_dir, "library_dir =", self.library_dir)
end

function CloudReader:saveSettings()
    self.settings:saveSetting("server_url", self.server_url)
    self.settings:saveSetting("username", self.username)
    self.settings:saveSetting("token", self.token)
    self.settings:saveSetting("user_id", self.user_id)
    self.settings:saveSetting("auto_sync", self.auto_sync)
    self.settings:saveSetting("library_dir", self.library_dir)
    self.settings:saveSetting("book_index", self.book_index)
    self.settings:flush()
end

function CloudReader:ensureLibraryDir()
    self:mkdirp(self.library_dir)
end

function CloudReader:mkdirp(path)
    logger.dbg("CloudReader: mkdirp called with:", path)
    
    local is_absolute = path:sub(1, 1) == "/"
    local current = is_absolute and "" or "."
    
    for dir in path:gmatch("[^/]+") do
        if current == "." then
            current = dir
        else
            current = current .. "/" .. dir
        end
        
        local mode = lfs.attributes(current, "mode")
        if mode ~= "directory" then
            local ok, err = lfs.mkdir(current)
            if ok then
                logger.dbg("CloudReader: Created dir:", current)
            elseif err and not err:match("exists") then
                logger.err("CloudReader: Failed to create dir:", current, "error:", err)
            end
        end
    end
end

function CloudReader:isLoggedIn()
    return self.token and self.token ~= ""
end

function CloudReader:getBaseUrl()
    local url = self.server_url or ""
    if not url:match("^https?://") then
        url = "http://" .. url
    end
    return url:gsub("/$", "")
end

-- Menu

function CloudReader:addToMainMenu(menu_items)
    if self:isLoggedIn() then
        menu_items.cloudreader = {
            text = _("CloudReader"),
            sorting_hint = "tools",
            callback = function()
                self:openLibrary()
            end,
            hold_callback = function()
                self:showSettingsMenu()
            end,
        }
    else
        menu_items.cloudreader = {
            text = _("CloudReader"),
            sorting_hint = "tools",
            sub_item_table = {
                {
                    text = _("Login / Register"),
                    keep_menu_open = true,
                    callback = function(touchmenu_instance)
                        self:showLoginDialog(touchmenu_instance)
                    end,
                },
                {
                    text_func = function()
                        return T(_("Server: %1"), self.server_url ~= "" and self.server_url or _("not set"))
                    end,
                    keep_menu_open = true,
                    callback = function(touchmenu_instance)
                        self:editServerUrl(touchmenu_instance)
                    end,
                },
            },
        }
    end
end

function CloudReader:showSettingsMenu()
    local ButtonDialog = require("ui/widget/buttondialog")
    
    self.settings_dialog = ButtonDialog:new{
        title = T(_("CloudReader - %1"), self.username),
        buttons = {
            {
                {
                    text = _("Open library folder"),
                    callback = function()
                        UIManager:close(self.settings_dialog)
                        self:openLibrary()
                    end,
                },
            },
            {
                {
                    text = _("Sync library now"),
                    callback = function()
                        UIManager:close(self.settings_dialog)
                        NetworkMgr:runWhenConnected(function()
                            self:syncLibrary()
                        end)
                    end,
                },
            },
            {
                {
                    text = _("Sync current book progress"),
                    callback = function()
                        UIManager:close(self.settings_dialog)
                        if self.ui.document then
                            NetworkMgr:runWhenConnected(function()
                                self:syncCurrentBook()
                            end)
                        else
                            UIManager:show(InfoMessage:new{ text = _("No book open.") })
                        end
                    end,
                },
            },
            {
                {
                    text = self.auto_sync and _("Auto-sync: ON") or _("Auto-sync: OFF"),
                    callback = function()
                        self.auto_sync = not self.auto_sync
                        self:saveSettings()
                        if self.auto_sync then
                            self:scheduleNextSync()
                        end
                        UIManager:close(self.settings_dialog)
                        self:showSettingsMenu()
                    end,
                },
            },
            {
                {
                    text = _("Logout"),
                    callback = function()
                        UIManager:close(self.settings_dialog)
                        self.token = ""
                        self.user_id = ""
                        self:saveSettings()
                        UIManager:show(InfoMessage:new{
                            text = _("Logged out."),
                            timeout = 2,
                        })
                    end,
                },
            },
            {
                {
                    text = _("Close"),
                    callback = function()
                        UIManager:close(self.settings_dialog)
                    end,
                },
            },
        },
    }
    UIManager:show(self.settings_dialog)
end

-- HTTP API

function CloudReader:callAPI(method, url_path, headers, body_json)
    if not self.server_url or self.server_url == "" then
        return false, "Server URL not configured", nil
    end
    
    local base_url = self:getBaseUrl()
    local full_url = base_url .. url_path
    
    local sink = {}
    local request = {
        url = full_url,
        method = method,
        sink = ltn12.sink.table(sink),
    }
    
    request.headers = headers or {}
    request.headers["Content-Type"] = request.headers["Content-Type"] or "application/json"
    request.headers["Accept"] = "application/json"
    
    if self.token and self.token ~= "" and not url_path:match("/auth/") then
        request.headers["Authorization"] = "Bearer " .. self.token
    end
    
    if body_json then
        request.source = ltn12.source.string(body_json)
        request.headers["Content-Length"] = tostring(#body_json)
    end
    
    logger.dbg("CloudReader:callAPI:", method, full_url)
    socketutil:set_timeout(TIMEOUT_BLOCK, TIMEOUT_TOTAL)
    
    local pcall_ok, code, resp_headers, status = pcall(function()
        return socket.skip(1, http.request(request))
    end)
    
    socketutil:reset_timeout()
    
    if not pcall_ok then
        logger.err("CloudReader:callAPI: Lua error:", code)
        return false, "Request error: " .. tostring(code), nil
    end
    
    if resp_headers == nil then
        logger.err("CloudReader:callAPI: network error", status or code)
        return false, "Network error", nil
    end
    
    local content = table.concat(sink)
    
    if code >= 200 and code < 300 then
        if content ~= "" then
            local ok, result = pcall(JSON.decode, content)
            if ok and result then
                return true, result, code
            end
        end
        return true, {}, code
    end
    
    local error_msg = "HTTP " .. tostring(code)
    if content ~= "" then
        local ok, data = pcall(JSON.decode, content)
        if ok and data and data.error then
            error_msg = data.error
        end
    end
    
    return false, error_msg, code
end

-- Login

function CloudReader:showLoginDialog(touchmenu_instance)
    self.login_dialog = MultiInputDialog:new{
        title = _("CloudReader Login"),
        fields = {
            {
                text = self.server_url,
                hint = _("Server URL (e.g. 192.168.1.100:8080)"),
            },
            {
                text = self.username,
                hint = _("Username"),
            },
            {
                text = "",
                hint = _("Password"),
                text_type = "password",
            },
        },
        buttons = {
            {
                {
                    text = _("Cancel"),
                    id = "close",
                    callback = function()
                        UIManager:close(self.login_dialog)
                    end,
                },
                {
                    text = _("Register"),
                    callback = function()
                        local fields = self.login_dialog:getFields()
                        UIManager:close(self.login_dialog)
                        NetworkMgr:runWhenConnected(function()
                            self:doAuth("register", fields[1], fields[2], fields[3], touchmenu_instance)
                        end)
                    end,
                },
                {
                    text = _("Login"),
                    is_enter_default = true,
                    callback = function()
                        local fields = self.login_dialog:getFields()
                        UIManager:close(self.login_dialog)
                        NetworkMgr:runWhenConnected(function()
                            self:doAuth("login", fields[1], fields[2], fields[3], touchmenu_instance)
                        end)
                    end,
                },
            },
        },
    }
    UIManager:show(self.login_dialog)
    self.login_dialog:onShowKeyboard()
end

function CloudReader:doAuth(action, server_url, username, password, touchmenu_instance)
    if server_url == "" or username == "" or password == "" then
        UIManager:show(InfoMessage:new{ text = _("Please fill all fields.") })
        return
    end
    
    server_url = server_url:gsub("/$", "")
    if not server_url:match("^https?://") then
        server_url = "http://" .. server_url
    end
    
    self.server_url = server_url
    self:saveSettings()
    
    local url_path = "/api/auth/" .. action
    local body = JSON.encode({
        username = username,
        password = password,
    })
    
    local ok, result = self:callAPI("POST", url_path, {
        ["Content-Type"] = "application/json",
    }, body)
    
    if ok and result then
        self.username = result.username or username
        self.token = result.token
        self.user_id = result.user_id
        self:saveSettings()
        
        UIManager:show(InfoMessage:new{
            text = T(_("Logged in as %1\n\nSyncing library..."), self.username),
            timeout = 2,
        })
        
        UIManager:scheduleIn(1, function()
            NetworkMgr:runWhenConnected(function()
                self:syncLibrary()
            end)
        end)
        
        if touchmenu_instance then
            touchmenu_instance:updateItems()
        end
    else
        UIManager:show(InfoMessage:new{
            text = T(_("%1 failed: %2"), action, result or _("Unknown error")),
        })
    end
end

function CloudReader:editServerUrl(touchmenu_instance)
    self.url_dialog = InputDialog:new{
        title = _("Server URL"),
        input = self.server_url,
        hint = "192.168.1.100:8080",
        buttons = {
            {
                {
                    text = _("Cancel"),
                    id = "close",
                    callback = function()
                        UIManager:close(self.url_dialog)
                    end,
                },
                {
                    text = _("Save"),
                    is_enter_default = true,
                    callback = function()
                        self.server_url = self.url_dialog:getInputText():gsub("/$", "")
                        self:saveSettings()
                        UIManager:close(self.url_dialog)
                        if touchmenu_instance then
                            touchmenu_instance:updateItems()
                        end
                    end,
                },
            },
        },
    }
    UIManager:show(self.url_dialog)
    self.url_dialog:onShowKeyboard()
end

-- Library Sync

function CloudReader:syncLibrary()
    UIManager:preventStandby()
    
    UIManager:show(InfoMessage:new{
        text = _("Syncing library..."),
        timeout = 1,
    })
    UIManager:forceRePaint()
    
    logger.info("CloudReader: Starting sync, library_dir =", self.library_dir)
    
    local ok, result = self:callAPI("GET", "/api/library")
    
    if not ok then
        logger.err("CloudReader: Sync API failed:", result)
        UIManager:show(InfoMessage:new{
            text = T(_("Sync failed: %1"), result or _("Unknown error")),
        })
        UIManager:allowStandby()
        return
    end
    
    logger.info("CloudReader: API response received")
    
    if not result.books then
        logger.warn("CloudReader: No books in response")
        UIManager:show(InfoMessage:new{
            text = _("No books found on server."),
        })
        UIManager:allowStandby()
        return
    end
    
    logger.info("CloudReader: Found", #result.books, "books")
    
    local created = 0
    local skipped = 0
    local errors = 0
    local total = #result.books
    
    self.book_index = {}
    
    for i, book in ipairs(result.books) do
        logger.dbg("CloudReader: Processing book", i, book.path)
        local success, was_created = self:createPlaceholder(book)
        if success then
            self.book_index[book.id] = book
            if was_created then
                created = created + 1
            else
                skipped = skipped + 1
            end
        else
            errors = errors + 1
            logger.err("CloudReader: Failed to create placeholder for", book.path)
        end
        
        if i % 50 == 0 then
            UIManager:show(InfoMessage:new{
                text = T(_("Syncing library... %1/%2"), i, total),
                timeout = 0.5,
            })
            UIManager:forceRePaint()
        end
    end
    
    self:saveSettings()
    logger.info("CloudReader: Sync done. Created:", created, "Skipped:", skipped, "Errors:", errors)
    UIManager:allowStandby()
    
    UIManager:show(InfoMessage:new{
        text = T(_("Library synced!\n\nTotal: %1 books\nNew: %2\nExisting: %3\nErrors: %4\n\nPath: %5"),
            #result.books, created, skipped, errors, self.library_dir),
        timeout = 5,
    })
end

function CloudReader:openLibrary()
    local FileManager = require("apps/filemanager/filemanager")
    
    if not lfs.attributes(self.library_dir, "mode") then
        self:ensureLibraryDir()
    end
    
    if FileManager.instance then
        FileManager.instance:reinit(self.library_dir)
    else
        FileManager:showFiles(self.library_dir)
    end
end

-- Placeholder

function CloudReader:createPlaceholder(book)
    local rel_path = book.path
    local full_path = self.library_dir .. "/" .. rel_path
    
    logger.dbg("CloudReader: createPlaceholder full_path =", full_path)
    
    local parent = full_path:match("(.+)/[^/]+$")
    if parent then
        logger.dbg("CloudReader: Creating parent dir:", parent)
        self:mkdirp(parent)
    end
    
    local attr = lfs.attributes(full_path)
    if attr and attr.size > 0 then
        logger.dbg("CloudReader: File already exists:", full_path)
        return true, false
    end
    
    local base_url = self:getBaseUrl()
    local placeholder_url = base_url .. "/books/" .. book.id .. "/placeholder"
    
    logger.dbg("CloudReader: Downloading placeholder from:", placeholder_url)
    
    local temp_path = full_path .. ".tmp"
    local file = io.open(temp_path, "wb")
    if not file then
        logger.err("CloudReader: Cannot create file:", temp_path)
        return false, false
    end
    
    socketutil:set_timeout(TIMEOUT_BLOCK, TIMEOUT_TOTAL)
    local code = socket.skip(1, http.request{
        url = placeholder_url,
        method = "GET",
        sink = ltn12.sink.file(file),
    })
    socketutil:reset_timeout()
    
    if code ~= 200 then
        logger.err("CloudReader: Failed to download placeholder, code:", code)
        os.remove(temp_path)
        return false, false
    end
    
    os.rename(temp_path, full_path)
    logger.info("CloudReader: Created placeholder:", full_path)
    return true, true
end

function CloudReader:isPlaceholder(file_path)
    local attr = lfs.attributes(file_path)
    if not attr then return false, nil end
    
    if attr.size > PLACEHOLDER_MAX_SIZE then
        return false, nil
    end
    
    local rel_path = nil
    if file_path:sub(1, #self.library_dir) == self.library_dir then
        rel_path = file_path:sub(#self.library_dir + 2)
    end
    
    if not rel_path then return false, nil end
    
    for book_id, book in pairs(self.book_index) do
        if book.path == rel_path then
            logger.info("CloudReader: Detected placeholder, book_id =", book_id)
            return true, book_id
        end
    end
    
    local file = io.open(file_path, "rb")
    if not file then
        return false, nil
    end
    
    local header = file:read(5)
    file:close()
    
    if header and header == "%PDF-" and attr.size < PLACEHOLDER_MAX_SIZE then
        logger.warn("CloudReader: Small PDF not in index, may be orphaned placeholder:", file_path)
    end
    
    return false, nil
end

function CloudReader:downloadBook(file_path, book_id, callback)
    local book = self.book_index[book_id]
    if not book then
        UIManager:show(InfoMessage:new{
            text = _("Book not found in index. Try syncing library."),
        })
        return
    end
    
    local base_url = self:getBaseUrl()
    local download_url = base_url .. "/books/" .. book_id .. "/download"
    
    UIManager:show(InfoMessage:new{
        text = T(_("Downloading:\n%1"), book.title),
        timeout = 2,
    })
    UIManager:forceRePaint()
    
    local temp_path = file_path .. ".tmp"
    local file = io.open(temp_path, "wb")
    if not file then
        UIManager:show(InfoMessage:new{ text = _("Cannot create file.") })
        return
    end
    
    local request = {
        url = download_url,
        method = "GET",
        sink = ltn12.sink.file(file),
        headers = {},
    }
    
    if self.token and self.token ~= "" then
        request.headers["Authorization"] = "Bearer " .. self.token
    end
    
    socketutil:set_timeout(TIMEOUT_DOWNLOAD_BLOCK, TIMEOUT_DOWNLOAD_TOTAL)
    
    local pcall_ok, code, headers = pcall(function()
        return socket.skip(1, http.request(request))
    end)
    
    socketutil:reset_timeout()
    
    if not pcall_ok or headers == nil or code ~= 200 then
        os.remove(temp_path)
        UIManager:show(InfoMessage:new{
            text = T(_("Download failed: %1"), code or _("error")),
        })
        return
    end
    
    os.remove(file_path)
    os.rename(temp_path, file_path)
    
    logger.info("CloudReader: Downloaded:", file_path)
    
    if callback then
        callback()
    end
end

-- FileManager Hook

function CloudReader:patchReaderUI()
    local FileManager = require("apps/filemanager/filemanager")
    
    if FileManager._cloudreader_patched then
        return
    end
    
    local original_openFile = FileManager.openFile
    local cloudreader = self
    
    FileManager.openFile = function(self_fm, file, provider)
        local is_placeholder, book_id = cloudreader:isPlaceholder(file)
        
        if is_placeholder and book_id then
            logger.info("CloudReader: Intercepted placeholder:", file, "book_id:", book_id)
            
            NetworkMgr:runWhenConnected(function()
                cloudreader:downloadBook(file, book_id, function()
                    original_openFile(self_fm, file, provider)
                end)
            end)
            return true
        end
        
        return original_openFile(self_fm, file, provider)
    end
    
    FileManager._cloudreader_patched = true
    logger.info("CloudReader: FileManager patched")
end

-- Document Events

function CloudReader:onReaderReady()
    if self.auto_sync and self:isLoggedIn() then
        UIManager:scheduleIn(2, function()
            NetworkMgr:runWhenConnected(function()
                self:fetchAndApplyProgress()
            end)
        end)
    end
end

function CloudReader:onCloseDocument()
    if self.auto_sync and self:isLoggedIn() and self.ui.document then
        local book_id = self:getBookId()
        if book_id then
            local progress = self:getProgressData()
            local body = JSON.encode(progress)
            self:callAPI("PUT", "/api/sync/progress/" .. book_id, nil, body)
        end
    end
end

-- Progress Sync

function CloudReader:syncCurrentBook()
    if not self.ui.document then
        UIManager:show(InfoMessage:new{ text = _("No book open.") })
        return
    end
    
    local book_id = self:getBookId()
    if not book_id then
        UIManager:show(InfoMessage:new{ text = _("Cannot identify book.") })
        return
    end
    
    local progress = self:getProgressData()
    local body = JSON.encode(progress)
    local ok, result = self:callAPI("PUT", "/api/sync/progress/" .. book_id, nil, body)
    
    if ok then
        UIManager:show(InfoMessage:new{
            text = T(_("Synced: %1%%"), math.floor(progress.percentage or 0)),
            timeout = 2,
        })
    else
        UIManager:show(InfoMessage:new{
            text = T(_("Sync failed: %1"), result or _("error")),
        })
    end
end

function CloudReader:getBookId()
    if not self.ui.document then return nil end
    
    local doc_settings = self.ui.doc_settings
    if doc_settings then
        local md5 = doc_settings:readSetting("partial_md5_checksum")
        if md5 and md5 ~= "" then return md5 end
    end
    
    local file_path = self.ui.document.file or ""
    local filename = file_path:match("([^/\\]+)$") or file_path
    local hash = 0
    for i = 1, #filename do
        hash = (hash * 31 + filename:byte(i)) % 2147483647
    end
    return string.format("%08x", hash)
end

function CloudReader:getProgressData()
    local data = { status = "reading" }
    
    if not self.ui.document then return data end
    
    local current_page = self.ui.document:getCurrentPage() or 0
    local total_pages = self.ui.document:getPageCount() or 0
    
    data.current_page = current_page
    data.total_pages = total_pages
    data.percentage = total_pages > 0 and (current_page / total_pages * 100) or 0
    
    local doc_settings = self.ui.doc_settings
    if doc_settings then
        local percent = doc_settings:readSetting("percent_finished")
        if percent then data.percentage = percent * 100 end
    end
    
    if data.percentage >= 99 then data.status = "complete" end
    
    return data
end

function CloudReader:fetchAndApplyProgress()
    local book_id = self:getBookId()
    if not book_id then return end
    
    local ok, result = self:callAPI("GET", "/api/sync/progress/" .. book_id)
    
    if ok and type(result) == "table" and result.percentage then
        local current = self:getProgressData().percentage
        
        if math.abs(result.percentage - current) > 2 then
            local ConfirmBox = require("ui/widget/confirmbox")
            UIManager:show(ConfirmBox:new{
                text = T(_("Server: %1%%\nLocal: %2%%\n\nJump to server position?"),
                    math.floor(result.percentage), math.floor(current)),
                ok_text = _("Yes"),
                ok_callback = function()
                    if result.current_page then
                        self.ui:handleEvent(Event:new("GotoPage", result.current_page))
                    end
                end,
            })
        end
    end
end

return CloudReader
