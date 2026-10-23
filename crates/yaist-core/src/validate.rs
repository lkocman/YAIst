//! Validators for parameters that end up in commands or D-Bus calls.

type R = Result<(), String>;

fn chars_ok(s: &str, extra: &str) -> bool {
    s.chars().all(|c| c.is_ascii_alphanumeric() || extra.contains(c))
}

pub fn username(s: &str) -> R {
    let first_ok = s.chars().next().is_some_and(|c| c.is_ascii_lowercase() || c == '_');
    if !first_ok || s.len() > 32 || !s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-') {
        return Err("use lowercase letters, digits, '_' or '-', starting with a letter (max 32)".into());
    }
    Ok(())
}

/// GECOS field: no colons or newlines.
pub fn full_name(s: &str) -> R {
    if s.contains([':', '\n', ',']) || s.len() > 128 {
        return Err("must not contain ':', ',' or newlines".into());
    }
    Ok(())
}

pub fn unit_name(s: &str) -> R {
    if s.is_empty() || s.len() > 255 || s.starts_with('-') || !chars_ok(s, ":_.@-\\") {
        return Err("not a valid systemd unit name".into());
    }
    Ok(())
}

/// A port or range: `8080` or `60000-61000`.
pub fn port(s: &str) -> R {
    let ok = |p: &str| p.parse::<u16>().is_ok_and(|n| n > 0);
    let valid = match s.split_once('-') {
        Some((a, b)) => ok(a) && ok(b),
        None => ok(s),
    };
    if valid {
        Ok(())
    } else {
        Err("expected a port 1-65535 or a range like 60000-61000".into())
    }
}

/// A firewall rule as shown in the panel: `8080/tcp` or a service name.
pub fn firewall_rule(s: &str) -> R {
    match s.split_once('/') {
        Some((p, proto)) => {
            port(p)?;
            if !["tcp", "udp", "sctp", "dccp"].contains(&proto) {
                return Err("protocol must be tcp, udp, sctp or dccp".into());
            }
            Ok(())
        }
        None => name(s),
    }
}

/// firewalld zone/service names, SELinux booleans, repository aliases.
pub fn name(s: &str) -> R {
    if s.is_empty() || s.len() > 128 || s.starts_with('-') || !chars_ok(s, "_.+-") {
        return Err("use letters, digits, '_', '.', '+' or '-'".into());
    }
    Ok(())
}

/// Package names, space separated.
pub fn packages(s: &str) -> R {
    if s.split_whitespace().next().is_none() {
        return Err("no package given".into());
    }
    for p in s.split_whitespace() {
        if p.starts_with('-') || !chars_ok(p, "_.+-") {
            return Err(format!("'{p}' is not a package name"));
        }
    }
    Ok(())
}

pub fn search_term(s: &str) -> R {
    if s.starts_with('-') || s.len() > 100 || !chars_ok(s, "_.+-* ") {
        return Err("use letters, digits and _ . + - *".into());
    }
    Ok(())
}

pub fn repo_url(s: &str) -> R {
    let ok_scheme = ["https://", "http://", "obs://"].iter().any(|p| s.starts_with(p));
    if !ok_scheme || s.chars().any(char::is_whitespace) {
        return Err("expected an https:// (or obs://) URL".into());
    }
    Ok(())
}

/// OBS project, e.g. `home:alice` or `devel:languages:rust`.
pub fn obs_project(s: &str) -> R {
    if s.is_empty() || s.starts_with(':') || s.contains("::") || !chars_ok(s, ":_.+-") {
        return Err("expected an OBS project like home:user or devel:tools".into());
    }
    Ok(())
}

pub fn hostname(s: &str) -> R {
    let label_ok = |l: &str| !l.is_empty() && l.len() <= 63 && !l.starts_with('-') && !l.ends_with('-') && chars_ok(l, "-");
    if s.len() > 253 || !s.split('.').all(label_ok) {
        return Err("letters, digits and '-', dot-separated labels".into());
    }
    Ok(())
}

pub fn timezone(s: &str) -> R {
    if s.contains("..") || s.starts_with('/') || !chars_ok(s, "/_+-") {
        return Err("expected a zone like Europe/Prague".into());
    }
    if !std::path::Path::new("/usr/share/zoneinfo").join(s).is_file() {
        return Err(format!("unknown time zone '{s}'"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_option_injection() {
        assert!(username("-oops").is_err());
        assert!(packages("vim --force").is_err());
        assert!(unit_name("--now").is_err());
        assert!(name("-x").is_err());
    }

    #[test]
    fn accepts_common_values() {
        assert!(username("alice").is_ok());
        assert!(unit_name("sshd.service").is_ok());
        assert!(unit_name("getty@tty1.service").is_ok());
        assert!(firewall_rule("8080/tcp").is_ok());
        assert!(firewall_rule("ssh").is_ok());
        assert!(port("60000-61000").is_ok());
        assert!(obs_project("devel:languages:rust").is_ok());
    }
}
