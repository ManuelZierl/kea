//! Shell-integration marker authentication.
//!
//! Kea's integration markers (OSC 777/778/779 `kea;…`) travel in the same byte
//! stream as arbitrary program output, so any program could print one. Live
//! sessions therefore require a random token that only Kea and the shell hook
//! it installed know: `ESC ] 777 ; kea ; <token> ; <kind> ; …`. Output without
//! the right token is ordinary output, never readiness, cwd or block structure.
//!
//! The token is a capability for interoperability, not a guarantee about the
//! foreground process: whatever runs in the integrated shell can read it.

/// Hex digits after the `k` prefix.
const TOKEN_HEX: usize = 32;

/// A marker token: `k` followed by 32 lowercase hex digits. The prefix keeps a
/// token distinguishable from every marker kind of the legacy format.
#[derive(Clone, Eq, PartialEq)]
pub struct MarkerToken(String);

impl MarkerToken {
    pub fn from_bytes(bytes: [u8; 16]) -> Self {
        let mut token = String::with_capacity(1 + TOKEN_HEX);
        token.push('k');
        for byte in bytes {
            token.push_str(&format!("{byte:02x}"));
        }
        Self(token)
    }

    pub fn parse(text: &str) -> Option<Self> {
        looks_like_token(text).then(|| Self(text.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for MarkerToken {
    // Never print the secret into logs or panic messages.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("MarkerToken(…)")
    }
}

fn looks_like_token(text: &str) -> bool {
    text.len() == 1 + TOKEN_HEX
        && text.starts_with('k')
        && text[1..]
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Who a marker may be attributed to.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MarkerOrigin {
    /// Carried this session's token: the shell Kea started and integrated.
    Session,
    /// Carried the user's remote-integration token (installed by the user on
    /// another host). Never the local shell.
    Remote,
    /// Replay of recorded output; nothing is executed or trusted from it.
    Replay,
}

/// Which markers a parser accepts.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum MarkerAuth {
    /// Recorded output: tokens are optional and not checked.
    #[default]
    Replay,
    /// A live session: markers must carry the session or remote token.
    Live {
        session: MarkerToken,
        remote: Option<MarkerToken>,
    },
}

impl MarkerAuth {
    /// Check the body after `kea;` and return the origin and the body after the
    /// token. `None` means the marker must be treated as ordinary output.
    pub fn check<'a>(&self, body: &'a str) -> Option<(MarkerOrigin, &'a str)> {
        let (first, rest) = body.split_once(';').unwrap_or((body, ""));
        let tokened = looks_like_token(first);
        match self {
            Self::Replay => Some((MarkerOrigin::Replay, if tokened { rest } else { body })),
            Self::Live { .. } if !tokened => None,
            Self::Live { session, .. } if session.as_str() == first => {
                Some((MarkerOrigin::Session, rest))
            }
            Self::Live {
                remote: Some(remote),
                ..
            } if remote.as_str() == first => Some((MarkerOrigin::Remote, rest)),
            Self::Live { .. } => None,
        }
    }
}

#[cfg(test)]
#[path = "../tests/unit/marker.rs"]
mod tests;
