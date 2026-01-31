/// Exit codes defined in DESIGN §9.
///
/// Each variant maps to a specific exit code. When `PLUCK_NO_RETURN_ERROR`
/// is set to a non-zero value, all error exits return 0 instead.
#[derive(Debug)]
pub enum ErrorCode {
    PluckingError,
    ConfigError,
    Internal(u32),
}

impl ErrorCode {
    /// Convert to a raw exit code, respecting `PLUCK_NO_RETURN_ERROR`.
    pub fn to_raw(&self) -> i32 {
        let code = match self {
            Self::PluckingError => 1,
            Self::ConfigError => 2,
            Self::Internal(c) => (*c).cast_signed(),
        };
        if no_return_error() { 0 } else { code }
    }
}

/// Check if `PLUCK_NO_RETURN_ERROR` is set to a non-zero, non-empty value.
fn no_return_error() -> bool {
    std::env::var("PLUCK_NO_RETURN_ERROR").is_ok_and(|v| !v.is_empty() && v != "0")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn test_exit_code_plucking_error() {
        assert_eq!(ErrorCode::PluckingError.to_raw(), 1);
    }

    #[test]
    fn test_exit_code_config_error() {
        assert_eq!(ErrorCode::ConfigError.to_raw(), 2);
    }

    #[test]
    fn test_exit_code_internal() {
        assert_eq!(ErrorCode::Internal(3).to_raw(), 3);
    }

    #[test]
    fn test_no_return_error_unset() {
        unsafe {
            env::remove_var("PLUCK_NO_RETURN_ERROR");
        }
        assert!(!no_return_error());
    }

    #[test]
    fn test_no_return_error_zero() {
        unsafe {
            env::set_var("PLUCK_NO_RETURN_ERROR", "0");
        }
        assert!(!no_return_error());
    }

    #[test]
    fn test_no_return_error_empty() {
        unsafe {
            env::set_var("PLUCK_NO_RETURN_ERROR", "");
        }
        assert!(!no_return_error());
    }

    #[test]
    fn test_no_return_error_nonzero() {
        unsafe {
            env::set_var("PLUCK_NO_RETURN_ERROR", "1");
        }
        assert!(no_return_error());
    }

    #[test]
    fn test_no_return_error_arbitrary() {
        unsafe {
            env::set_var("PLUCK_NO_RETURN_ERROR", "true");
        }
        assert!(no_return_error());
    }

    #[test]
    fn test_exit_code_respects_no_return_error() {
        unsafe {
            env::set_var("PLUCK_NO_RETURN_ERROR", "1");
        }
        assert_eq!(ErrorCode::PluckingError.to_raw(), 0);
        assert_eq!(ErrorCode::ConfigError.to_raw(), 0);
        assert_eq!(ErrorCode::Internal(3).to_raw(), 0);
        unsafe {
            env::remove_var("PLUCK_NO_RETURN_ERROR");
        }
    }
}
