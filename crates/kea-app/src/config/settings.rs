//! Application-owned settings. Missing values inherit platform/component defaults.
use anyhow::{Context as _, Result};
use std::{
    env, fs,
    io::Write as _,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Appearance {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum PostSubmitFocus {
    /// Hand keyboard ownership to the terminal application after a successful submit.
    Terminal,
    /// Keep the normal editor as the active surface so drafting the next command/prompt
    /// can continue while output streams above it. This is Kea's composer-first default.
    #[default]
    Editor,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub appearance: Appearance,
    pub font_family: Option<String>,
    pub font_size: Option<f32>,
    pub line_numbers: bool,
    pub soft_wrap: bool,
    pub output_wrap: bool,
    pub syntax_highlighting: bool,
    pub show_blocks: bool,
    pub post_submit_focus: PostSubmitFocus,
    /// Keep submitted drafts across restarts in a plaintext owner-only file.
    /// Off by default: submissions can contain secrets.
    pub persist_history: bool,
    pub shift_mouse_selects_locally: bool,
    /// Allow the decorative composer bird to animate after draft changes.
    pub animate_logo: bool,
    /// Persist successful compose submissions; explicit saved memories always persist.
    pub history_persistence: bool,
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
            show_blocks: false,
            post_submit_focus: PostSubmitFocus::Editor,
            persist_history: false,
            shift_mouse_selects_locally: true,
            animate_logo: true,
            history_persistence: false,
        }
    }
}

impl Settings {
    pub fn path() -> Option<PathBuf> {
        env::var_os("KEA_SETTINGS").map(Into::into).or_else(|| {
            crate::config::keybindings::config_path()
                .map(|path| path.with_file_name("settings.conf"))
        })
    }

    pub fn load() -> (Self, Option<String>) {
        let Some(path) = Self::path() else {
            return (Self::default(), None);
        };
        match fs::read_to_string(&path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                // Leave a commented starting point behind, like keybindings.
                if let Err(error) = crate::config::keybindings::ensure_default_file(
                    &path,
                    crate::config::keybindings::DEFAULT_SETTINGS_CONF,
                ) {
                    return (
                        Self::default(),
                        Some(format!(
                            "Settings {} unavailable: {error}. Using system defaults.",
                            path.display()
                        )),
                    );
                }
                (Self::default(), None)
            }
            result => match result
                .map_err(anyhow::Error::from)
                .and_then(|text| Self::parse(&text))
            {
                Ok(settings) => (settings, None),
                Err(error) => (
                    Self::default(),
                    Some(format!(
                        "Settings {} ignored: {error}. Using system defaults.",
                        path.display()
                    )),
                ),
            },
        }
    }

    pub fn parse(text: &str) -> Result<Self> {
        let mut settings = Self::default();
        for (number, line) in text.lines().enumerate() {
            let line = line.split('#').next().unwrap_or_default().trim();
            if line.is_empty() {
                continue;
            }
            let (key, value) = line
                .split_once('=')
                .with_context(|| format!("line {} needs key = value", number + 1))?;
            let value = value.trim();
            match key.trim() {
                "theme" => {
                    settings.appearance = match value {
                        "system" => Appearance::System,
                        "light" => Appearance::Light,
                        "dark" => Appearance::Dark,
                        _ => anyhow::bail!("theme must be system, light or dark"),
                    }
                }
                "font_family" if value == "system" => settings.font_family = None,
                "font_family" if !value.is_empty() => settings.font_family = Some(value.into()),
                "font_size" if value == "system" => settings.font_size = None,
                "font_size" => {
                    let size: f32 = value.parse()?;
                    anyhow::ensure!(
                        (9.0..=40.0).contains(&size),
                        "font_size must be between 9 and 40"
                    );
                    settings.font_size = Some(size);
                }
                "line_numbers" => settings.line_numbers = boolean(value)?,
                "soft_wrap" => settings.soft_wrap = boolean(value)?,
                "output_wrap" => settings.output_wrap = boolean(value)?,
                "syntax_highlighting" => settings.syntax_highlighting = boolean(value)?,
                "show_blocks" => settings.show_blocks = boolean(value)?,
                "persist_history" => settings.persist_history = boolean(value)?,
                "shift_mouse_selects_locally" => {
                    settings.shift_mouse_selects_locally = boolean(value)?
                }
                "animate_logo" => settings.animate_logo = boolean(value)?,
                "post_submit_focus" => {
                    settings.post_submit_focus = match value {
                        "terminal" => PostSubmitFocus::Terminal,
                        "editor" => PostSubmitFocus::Editor,
                        _ => anyhow::bail!("post_submit_focus must be terminal or editor"),
                    }
                }
                "history_persistence" => settings.history_persistence = boolean(value)?,
                unknown => anyhow::bail!("unknown setting `{unknown}`"),
            }
        }
        Ok(settings)
    }

    /// Persist a complete settings snapshot through a sibling temporary file so
    /// an interrupted write cannot truncate the previous configuration.
    pub fn save(&self) -> Result<PathBuf> {
        let path = Self::path().context("the settings directory is unavailable on this system")?;
        self.save_to(&path)?;
        Ok(path)
    }

    fn save_to(&self, path: &Path) -> Result<()> {
        let parent = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent)
            .with_context(|| format!("cannot create settings directory {}", parent.display()))?;

        let mut temporary = tempfile::Builder::new()
            .prefix(".settings-")
            .tempfile_in(parent)
            .with_context(|| format!("cannot create a temporary file in {}", parent.display()))?;
        temporary
            .write_all(self.to_config().as_bytes())
            .context("cannot write settings")?;
        temporary
            .as_file()
            .sync_all()
            .context("cannot flush settings")?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            temporary
                .as_file()
                .set_permissions(fs::Permissions::from_mode(0o600))
                .context("cannot protect settings permissions")?;
        }
        temporary
            .persist(path)
            .map_err(|error| error.error)
            .with_context(|| format!("cannot replace {}", path.display()))?;
        Ok(())
    }

    fn to_config(&self) -> String {
        let appearance = match self.appearance {
            Appearance::System => "system",
            Appearance::Light => "light",
            Appearance::Dark => "dark",
        };
        let post_submit_focus = match self.post_submit_focus {
            PostSubmitFocus::Terminal => "terminal",
            PostSubmitFocus::Editor => "editor",
        };
        let font_family = self.font_family.as_deref().unwrap_or("system");
        let font_size = self
            .font_size
            .map(|size| size.to_string())
            .unwrap_or_else(|| "system".into());
        format!(
            "# Kea settings. Changes made in the app are written here.\n\
theme = {appearance}\n\
post_submit_focus = {post_submit_focus}\n\
persist_history = {}\n\
history_persistence = {}\n\
shift_mouse_selects_locally = {}\n\
animate_logo = {}\n\
show_blocks = {}\n\
font_family = {font_family}\n\
font_size = {font_size}\n\
syntax_highlighting = {}\n\
line_numbers = {}\n\
soft_wrap = {}\n\
output_wrap = {}\n",
            self.persist_history,
            self.history_persistence,
            self.shift_mouse_selects_locally,
            self.animate_logo,
            self.show_blocks,
            self.syntax_highlighting,
            self.line_numbers,
            self.soft_wrap,
            self.output_wrap,
        )
    }
}

fn boolean(value: &str) -> Result<bool> {
    match value {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => anyhow::bail!("expected true or false"),
    }
}

#[cfg(test)]
#[path = "../../tests/unit/config/settings.rs"]
mod tests;
