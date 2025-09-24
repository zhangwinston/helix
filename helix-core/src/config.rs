use crate::syntax::{config::Configuration, Loader, LoaderError};

/// Language configuration based on built-in languages.toml.
pub fn default_lang_config() -> Configuration {
    let mut config: Configuration = helix_loader::config::default_lang_config()
        .try_into()
        .expect("Could not deserialize built-in languages.toml");

    // Apply smart defaults for IME settings
    for lang_config in &mut config.language {
        // Only apply defaults if no explicit user config is set
        if lang_config.auto_ime_scopes.is_empty() {
            if lang_config.comment_tokens.is_some() {
                lang_config.auto_ime_scopes = vec!["string".to_string(), "comment".to_string()];
            }
        }
    }
    config
}

/// Language configuration loader based on built-in languages.toml.
pub fn default_lang_loader() -> Loader {
    Loader::new(default_lang_config()).expect("Could not compile loader for default config")
}

#[derive(Debug)]
pub enum LanguageLoaderError {
    DeserializeError(toml::de::Error),
    LoaderError(LoaderError),
}

impl std::fmt::Display for LanguageLoaderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DeserializeError(err) => write!(f, "Failed to parse language config: {err}"),
            Self::LoaderError(err) => write!(f, "Failed to compile language config: {err}"),
        }
    }
}

impl std::error::Error for LanguageLoaderError {}

/// Language configuration based on user configured languages.toml.
pub fn user_lang_config() -> Result<Configuration, toml::de::Error> {
    helix_loader::config::user_lang_config()?.try_into()
}

/// Language configuration loader based on user configured languages.toml.
pub fn user_lang_loader() -> Result<Loader, LanguageLoaderError> {
    let mut config: Configuration = helix_loader::config::user_lang_config()
        .map_err(LanguageLoaderError::DeserializeError)?
        .try_into()
        .map_err(LanguageLoaderError::DeserializeError)?;

    // Apply smart defaults for IME settings
    for lang_config in &mut config.language {
        // Only apply defaults if no explicit user config is set
        if lang_config.auto_ime_scopes.is_empty() {
            if lang_config.comment_tokens.is_some() {
                lang_config.auto_ime_scopes = vec!["string".to_string(), "comment".to_string()];
            }
        }
    }
    Loader::new(config).map_err(LanguageLoaderError::LoaderError)
}
