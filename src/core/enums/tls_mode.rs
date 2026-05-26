#[derive(Debug, Clone, Default)]
#[cfg_attr(test, derive(PartialEq))]
pub enum TlsMode {
    #[default]
    Disable,
    Require,
    VerifyFull,
}

impl std::fmt::Display for TlsMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TlsMode::Disable => write!(f, "disable"),
            TlsMode::Require => write!(f, "require"),
            TlsMode::VerifyFull => write!(f, "verify-full"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tls_mode_displays_disable() {
        assert_eq!(TlsMode::Disable.to_string(), "disable");
    }

    #[test]
    fn tls_mode_displays_require() {
        assert_eq!(TlsMode::Require.to_string(), "require");
    }

    #[test]
    fn tls_mode_displays_verify_full() {
        assert_eq!(TlsMode::VerifyFull.to_string(), "verify-full");
    }
}
