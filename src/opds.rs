//! OPDS catalog generation.

use crate::library::Book;
use chrono::{DateTime, Utc};
use quick_xml::Writer;
use quick_xml::events::{BytesDecl, BytesEnd, BytesStart, BytesText, Event};
use serde::{Deserialize, Serialize};
use std::io::Cursor;

/// OPDS feed link.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Link {
    /// Link relation type (e.g., "self", "subsection", "acquisition").
    pub rel: String,
    /// URL of the linked resource.
    pub href: String,
    /// MIME type of the linked resource.
    pub link_type: String,
    /// Optional title for the link.
    pub title: Option<String>,
}

/// OPDS feed entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    /// Unique identifier for the entry.
    pub id: String,
    /// Entry title.
    pub title: String,
    /// Last update timestamp.
    pub updated: DateTime<Utc>,
    /// Authors list.
    pub authors: Vec<String>,
    /// Short summary text.
    pub summary: Option<String>,
    /// Full content/description.
    pub content: Option<String>,
    /// Links associated with this entry.
    pub links: Vec<Link>,
    /// Categories/tags.
    pub categories: Vec<String>,
}

/// OPDS feed builder.
pub struct FeedBuilder {
    id: String,
    title: String,
    updated: DateTime<Utc>,
    author_name: Option<String>,
    links: Vec<Link>,
    entries: Vec<Entry>,
}

impl FeedBuilder {
    /// Create a new feed builder.
    pub fn new(id: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            updated: Utc::now(),
            author_name: None,
            links: Vec::new(),
            entries: Vec::new(),
        }
    }

    /// Set the feed author.
    pub fn author(mut self, name: impl Into<String>) -> Self {
        self.author_name = Some(name.into());
        self
    }

    /// Add a self link.
    pub fn self_link(mut self, href: impl Into<String>) -> Self {
        self.links.push(Link {
            rel: "self".to_string(),
            href: href.into(),
            link_type: "application/atom+xml;profile=opds-catalog".to_string(),
            title: None,
        });
        self
    }

    /// Add a start link.
    pub fn start_link(mut self, href: impl Into<String>) -> Self {
        self.links.push(Link {
            rel: "start".to_string(),
            href: href.into(),
            link_type: "application/atom+xml;profile=opds-catalog".to_string(),
            title: None,
        });
        self
    }

    /// Add a search link.
    pub fn search_link(mut self, href: impl Into<String>) -> Self {
        self.links.push(Link {
            rel: "search".to_string(),
            href: href.into(),
            link_type: "application/opensearchdescription+xml".to_string(),
            title: None,
        });
        self
    }

    /// Add a navigation entry.
    pub fn navigation_entry(mut self, entry: Entry) -> Self {
        self.entries.push(entry);
        self
    }

    /// Add a book entry.
    pub fn book_entry(mut self, book: &Book, base_url: &str) -> Self {
        let mut links = vec![
            Link {
                rel: "http://opds-spec.org/acquisition".to_string(),
                href: format!("{}/books/{}/download", base_url, book.id),
                link_type: book.format.mime_type().to_string(),
                title: Some("Download".to_string()),
            },
            Link {
                rel: "http://opds-spec.org/image".to_string(),
                href: format!("{}/books/{}/cover", base_url, book.id),
                link_type: "image/png".to_string(),
                title: None,
            },
            Link {
                rel: "http://opds-spec.org/image/thumbnail".to_string(),
                href: format!("{}/books/{}/thumbnail", base_url, book.id),
                link_type: "image/png".to_string(),
                title: None,
            },
        ];

        // Add series link if available
        if book.series.is_some() {
            links.push(Link {
                rel: "related".to_string(),
                href: format!(
                    "{}/catalog/search?q={}",
                    base_url,
                    urlencoding::encode(book.series.as_ref().unwrap_or(&String::new()))
                ),
                link_type: "application/atom+xml;profile=opds-catalog".to_string(),
                title: Some("Series".to_string()),
            });
        }

        let entry = Entry {
            id: format!("urn:uuid:{}", book.id),
            title: book.title.clone(),
            updated: book.modified,
            authors: book.authors.clone(),
            summary: book.description.clone(),
            content: None,
            links,
            categories: book.tags.clone(),
        };

        self.entries.push(entry);
        self
    }

    /// Add a category entry.
    pub fn category_entry(mut self, category: &crate::library::Category, base_url: &str) -> Self {
        let entry = Entry {
            id: format!("urn:uuid:{}", category.id),
            title: category.name.clone(),
            updated: Utc::now(),
            authors: Vec::new(),
            summary: Some(format!(
                "{} books, {} subcategories",
                category.book_count, category.subcategory_count
            )),
            content: None,
            links: vec![Link {
                rel: "subsection".to_string(),
                href: format!("{}/catalog/category/{}", base_url, category.id),
                link_type: "application/atom+xml;profile=opds-catalog;kind=acquisition".to_string(),
                title: Some(category.name.clone()),
            }],
            categories: Vec::new(),
        };

        self.entries.push(entry);
        self
    }

    /// Build the XML feed.
    pub fn build(self) -> String {
        let mut writer = Writer::new(Cursor::new(Vec::new()));

        // XML declaration - writing to Vec can't fail
        let _ = writer.write_event(Event::Decl(BytesDecl::new("1.0", Some("UTF-8"), None)));

        // Feed element
        let mut feed = BytesStart::new("feed");
        feed.push_attribute(("xmlns", "http://www.w3.org/2005/Atom"));
        feed.push_attribute(("xmlns:opds", "http://opds-spec.org/2010/catalog"));
        feed.push_attribute(("xmlns:dc", "http://purl.org/dc/elements/1.1/"));
        let _ = writer.write_event(Event::Start(feed));

        // ID
        write_text_element(&mut writer, "id", &self.id);

        // Title
        write_text_element(&mut writer, "title", &self.title);

        // Updated
        write_text_element(&mut writer, "updated", &self.updated.to_rfc3339());

        // Author
        if let Some(name) = &self.author_name {
            let _ = writer.write_event(Event::Start(BytesStart::new("author")));
            write_text_element(&mut writer, "name", name);
            let _ = writer.write_event(Event::End(BytesEnd::new("author")));
        }

        // Links
        for link in &self.links {
            write_link(&mut writer, link);
        }

        // Entries
        for entry in &self.entries {
            write_entry(&mut writer, entry);
        }

        // Close feed
        let _ = writer.write_event(Event::End(BytesEnd::new("feed")));

        String::from_utf8(writer.into_inner().into_inner()).unwrap_or_default()
    }
}

/// Write a simple text element.
fn write_text_element<W: std::io::Write>(writer: &mut Writer<W>, name: &str, text: &str) {
    let _ = writer.write_event(Event::Start(BytesStart::new(name)));
    let _ = writer.write_event(Event::Text(BytesText::new(text)));
    let _ = writer.write_event(Event::End(BytesEnd::new(name)));
}

/// Write a link element.
fn write_link<W: std::io::Write>(writer: &mut Writer<W>, link: &Link) {
    let mut elem = BytesStart::new("link");
    elem.push_attribute(("rel", link.rel.as_str()));
    elem.push_attribute(("href", link.href.as_str()));
    elem.push_attribute(("type", link.link_type.as_str()));
    if let Some(title) = &link.title {
        elem.push_attribute(("title", title.as_str()));
    }
    let _ = writer.write_event(Event::Empty(elem));
}

/// Write an entry element.
fn write_entry<W: std::io::Write>(writer: &mut Writer<W>, entry: &Entry) {
    let _ = writer.write_event(Event::Start(BytesStart::new("entry")));

    write_text_element(writer, "id", &entry.id);
    write_text_element(writer, "title", &entry.title);
    write_text_element(writer, "updated", &entry.updated.to_rfc3339());

    // Authors
    for author in &entry.authors {
        let _ = writer.write_event(Event::Start(BytesStart::new("author")));
        write_text_element(writer, "name", author);
        let _ = writer.write_event(Event::End(BytesEnd::new("author")));
    }

    // Summary
    if let Some(summary) = &entry.summary {
        let mut elem = BytesStart::new("summary");
        elem.push_attribute(("type", "text"));
        let _ = writer.write_event(Event::Start(elem));
        let _ = writer.write_event(Event::Text(BytesText::new(summary)));
        let _ = writer.write_event(Event::End(BytesEnd::new("summary")));
    }

    // Content
    if let Some(content) = &entry.content {
        let mut elem = BytesStart::new("content");
        elem.push_attribute(("type", "html"));
        let _ = writer.write_event(Event::Start(elem));
        let _ = writer.write_event(Event::Text(BytesText::new(content)));
        let _ = writer.write_event(Event::End(BytesEnd::new("content")));
    }

    // Links
    for link in &entry.links {
        write_link(writer, link);
    }

    // Categories
    for category in &entry.categories {
        let mut elem = BytesStart::new("category");
        elem.push_attribute(("term", category.as_str()));
        elem.push_attribute(("label", category.as_str()));
        let _ = writer.write_event(Event::Empty(elem));
    }

    let _ = writer.write_event(Event::End(BytesEnd::new("entry")));
}

/// Generate OpenSearch description XML.
pub fn generate_opensearch(title: &str, base_url: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<OpenSearchDescription xmlns="http://a9.com/-/spec/opensearch/1.1/">
  <ShortName>{}</ShortName>
  <Description>Search the {} catalog</Description>
  <InputEncoding>UTF-8</InputEncoding>
  <OutputEncoding>UTF-8</OutputEncoding>
  <Url type="application/atom+xml;profile=opds-catalog" template="{}/catalog/search?q={{searchTerms}}"/>
</OpenSearchDescription>"#,
        title, title, base_url
    )
}
