//! Application-owned settings. Missing values inherit platform/component defaults.
use anyhow::{Context as _, Result};
use std::{env, fs};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Appearance {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Clone, Debug)]
pub struct Settings {
    pub appearance: Appearance,
    pub font_family: Option<String>,
    pub font_size: Option<f32>,
    pub line_numbers: bool,
    pub soft_wrap: bool,
    pub output_wrap: bool,
    pub syntax_highlighting: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            appearance: Appearance::System,
            font_family: None,
            font_size: None,
            line_numbers: false,
            soft_wrap: true,
            output_wrap: true,
            syntax_highlighting: true,
        }
    }
}

impl Settings {
    pub fn load() -> (Self, Option<String>) {
        let path = env::var_os("KEA_SETTINGS").map(Into::into).or_else(|| {
            crate::keybindings::config_path().map(|path| path.with_file_name("settings.conf"))
        });
        let Some(path) = path else { return (Self::default(), None) };
        match fs::read_to_string(&path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (Self::default(), None),
            result => match result.map_err(anyhow::Error::from).and_then(|text| Self::parse(&text)) {
                Ok(settings) => (settings, None),
                Err(error) => (Self::default(), Some(format!(
                    "Settings {} ignored: {error}. Using system defaults.", path.display()))),
            },
        }
    }

    pub fn parse(text: &str) -> Result<Self> {
        let mut settings = Self::default();
        for (number, line) in text.lines().enumerate() {
            let line = line.split('#').next().unwrap_or_default().trim();
            if line.is_empty() { continue }
            let (key, value) = line.split_once('=').with_context(|| format!("line {} needs key = value", number + 1))?;
            let value = value.trim();
            match key.trim() {
                "theme" => settings.appearance = match value {
                    "system" => Appearance::System, "light" => Appearance::Light, "dark" => Appearance::Dark,
                    _ => anyhow::bail!("theme must be system, light or dark"),
                },
                "font_family" if value == "system" => settings.font_family = None,
                "font_family" if !value.is_empty() => settings.font_family = Some(value.into()),
                "font_size" if value == "system" => settings.font_size = None,
                "font_size" => {
                    let size: f32 = value.parse()?;
                    anyhow::ensure!((9.0..=40.0).contains(&size), "font_size must be between 9 and 40");
                    settings.font_size = Some(size);
                },
                "line_numbers" => settings.line_numbers = boolean(value)?,
                "soft_wrap" => settings.soft_wrap = boolean(value)?,
                "output_wrap" => settings.output_wrap = boolean(value)?,
                "syntax_highlighting" => settings.syntax_highlighting = boolean(value)?,
                unknown => anyhow::bail!("unknown setting `{unknown}`"),
            }
        }
        Ok(settings)
    }
}

fn boolean(value: &str) -> Result<bool> {
    match value { "true" => Ok(true), "false" => Ok(false), _ => anyhow::bail!("expected true or false") }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn defaults_follow_system_and_overrides_are_explicit() {
        let settings = Settings::parse("theme = dark\nfont_size = 16\nsoft_wrap = false\n").unwrap();
        assert_eq!(settings.appearance, Appearance::Dark);
        assert_eq!(settings.font_size, Some(16.));
        assert!(!settings.soft_wrap);
        assert_eq!(Settings::default().appearance, Appearance::System);
        assert!(Settings::default().font_family.is_none());
    }
    #[test]
    fn rejects_unknown_and_unsafe_sizes() {
        for text in ["theme = purple", "font_size = NaN", "font_size = 100", "typo = true"] {
            assert!(Settings::parse(text).is_err());
        }
    }
}
