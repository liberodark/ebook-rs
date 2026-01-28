//! Authentication module.

use crate::db::{Database, Session, User, now_timestamp};
use crate::error::{AppError, Result};
use argon2::{
    Argon2,
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString, rand_core::OsRng},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use rand::RngCore;

/// Hash a password using Argon2.
pub fn hash_password(password: &str) -> Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();

    argon2
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|e| AppError::Internal(format!("Failed to hash password: {}", e)))
}

/// Verify a password against a hash.
pub fn verify_password(password: &str, hash: &str) -> Result<bool> {
    let parsed_hash = PasswordHash::new(hash)
        .map_err(|e| AppError::Internal(format!("Invalid password hash: {}", e)))?;

    Ok(Argon2::default()
        .verify_password(password.as_bytes(), &parsed_hash)
        .is_ok())
}

/// Generate a secure random token.
pub fn generate_token() -> String {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

/// Authentication service.
pub struct AuthService {
    db: Database,
    session_duration_days: u32,
    registration_enabled: bool,
}

impl AuthService {
    /// Create a new auth service.
    pub fn new(db: Database, session_duration_days: u32, registration_enabled: bool) -> Self {
        Self {
            db,
            session_duration_days,
            registration_enabled,
        }
    }

    /// Register a new user.
    pub fn register(&self, username: &str, password: &str) -> Result<User> {
        if !self.registration_enabled {
            return Err(AppError::InvalidFormat(
                "Registration is disabled".to_string(),
            ));
        }

        self.create_user(username, password, "user")
    }

    /// Create a new user (admin function).
    pub fn create_user(&self, username: &str, password: &str, role: &str) -> Result<User> {
        // Validate username
        if username.is_empty() || username.len() > 64 {
            return Err(AppError::InvalidFormat(
                "Username must be 1-64 characters".to_string(),
            ));
        }

        if !username
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
        {
            return Err(AppError::InvalidFormat(
                "Username can only contain letters, numbers, _ and -".to_string(),
            ));
        }

        // Validate password
        if password.len() < 4 {
            return Err(AppError::InvalidFormat(
                "Password must be at least 4 characters".to_string(),
            ));
        }

        // Validate role
        if role != "admin" && role != "user" {
            return Err(AppError::InvalidFormat(
                "Role must be 'admin' or 'user'".to_string(),
            ));
        }

        let password_hash = hash_password(password)?;

        let user = User {
            id: uuid::Uuid::new_v4().to_string(),
            username: username.to_string(),
            password_hash,
            display_name: None,
            role: role.to_string(),
            created_at: now_timestamp(),
            last_login: None,
        };

        self.db.create_user(&user)?;
        Ok(user)
    }

    /// Login and create a session.
    pub fn login(
        &self,
        username: &str,
        password: &str,
        device_id: Option<String>,
    ) -> Result<(User, String)> {
        let user = self
            .db
            .get_user_by_username(username)?
            .ok_or_else(|| AppError::InvalidFormat("Invalid username or password".to_string()))?;

        if !verify_password(password, &user.password_hash)? {
            return Err(AppError::InvalidFormat(
                "Invalid username or password".to_string(),
            ));
        }

        // Update last login
        self.db.update_user_last_login(&user.id)?;

        // Create session
        let token = generate_token();
        let expires_at = now_timestamp() + (self.session_duration_days as i64 * 24 * 60 * 60);

        let session = Session {
            token: token.clone(),
            user_id: user.id.clone(),
            device_id,
            expires_at,
        };

        self.db.create_session(&session)?;

        Ok((user, token))
    }

    /// Validate a session token and return the user.
    pub fn validate_token(&self, token: &str) -> Result<Option<User>> {
        let session = match self.db.get_session(token)? {
            Some(s) => s,
            None => return Ok(None),
        };

        // Check expiration
        if session.expires_at < now_timestamp() {
            self.db.delete_session(token)?;
            return Ok(None);
        }

        self.db.get_user_by_id(&session.user_id)
    }

    /// Logout (delete session).
    pub fn logout(&self, token: &str) -> Result<()> {
        self.db.delete_session(token)
    }

    /// Change user password.
    pub fn change_password(&self, username: &str, new_password: &str) -> Result<bool> {
        if new_password.len() < 4 {
            return Err(AppError::InvalidFormat(
                "Password must be at least 4 characters".to_string(),
            ));
        }

        let password_hash = hash_password(new_password)?;
        self.db.update_user_password(username, &password_hash)
    }

    /// Delete a user.
    pub fn delete_user(&self, username: &str) -> Result<bool> {
        self.db.delete_user(username)
    }

    /// List all users.
    pub fn list_users(&self) -> Result<Vec<User>> {
        self.db.list_users()
    }

    /// Check if a user is admin.
    pub fn is_admin(&self, user: &User) -> bool {
        user.role == "admin"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_password_hash_and_verify() {
        let password = "test_password_123";
        let hash = hash_password(password).unwrap();

        assert!(verify_password(password, &hash).unwrap());
        assert!(!verify_password("wrong_password", &hash).unwrap());
    }

    #[test]
    fn test_generate_token() {
        let token1 = generate_token();
        let token2 = generate_token();

        assert_eq!(token1.len(), 43); // Base64 of 32 bytes
        assert_ne!(token1, token2);
    }
}
