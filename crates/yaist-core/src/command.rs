//! The `/command` syntax: `/<module> <verb> [positional…] [name=value…]`,
//! e.g. `/firewall open-port 8080 tcp` or `/users create alice admin=yes`.
//! Every action has one, and panels echo the command for what they did.

use anyhow::{anyhow, bail, Result};
use serde_json::{Map, Value};

use crate::{ActionSpec, Args, Registry};

pub enum Parsed {
    Action { spec: ActionSpec, raw: Map<String, Value> },
    Help(Option<String>),
}

pub fn parse(registry: &Registry, line: &str) -> Result<Parsed> {
    let line = line.trim().strip_prefix('/').ok_or_else(|| anyhow!("commands start with '/'"))?;
    let words = split_words(line)?;
    let mut it = words.into_iter();
    let module = it.next().unwrap_or_default();
    if module.is_empty() || module == "help" || module == "?" {
        return Ok(Parsed::Help(it.next()));
    }
    let Some(verb) = it.next() else {
        if registry.find_module(&module).is_some() {
            return Ok(Parsed::Help(Some(module)));
        }
        bail!("unknown module '{module}' — try /help");
    };
    let id = format!("{module}.{}", verb.replace('-', "_"));
    let spec = *registry.get(&id).ok_or_else(|| anyhow!("unknown command '/{module} {verb}' — try /help {module}"))?;

    let mut raw = Map::new();
    let mut positional = Vec::new();
    for word in it {
        match word.split_once('=') {
            Some((k, v)) if spec.param(k).is_some() => {
                raw.insert(k.to_owned(), Value::String(v.to_owned()));
            }
            _ => positional.push(word),
        }
    }
    let free: Vec<&str> = spec.params.iter().map(|p| p.name).filter(|n| !raw.contains_key(*n)).collect();
    let mut free = free.into_iter();
    for word in positional {
        let p = free.next().ok_or_else(|| anyhow!("too many arguments for /{module} {verb}"))?;
        raw.insert(p.to_owned(), Value::String(word));
    }
    Ok(Parsed::Action { spec, raw })
}

/// The command a user would type to run `spec` with `args`.
pub fn format(spec: &ActionSpec, args: &Args) -> String {
    let mut out = format!("/{} {}", spec.module(), spec.verb().replace('_', "-"));
    let mut positional = true;
    for p in spec.params {
        let Some(v) = args.0.get(p.name) else {
            positional = false;
            continue;
        };
        let text = match v {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        if !p.required && p.default == Some(text.as_str()) {
            positional = false;
            continue;
        }
        positional &= p.required;
        out.push(' ');
        if !positional {
            out.push_str(p.name);
            out.push('=');
        }
        out.push_str(&quote(&text));
    }
    out
}

fn quote(s: &str) -> String {
    if !s.is_empty() && !s.chars().any(|c| c.is_whitespace() || c == '"' || c == '\'') {
        return s.to_owned();
    }
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

fn split_words(line: &str) -> Result<Vec<String>> {
    let mut words = Vec::new();
    let mut cur = String::new();
    let mut in_word = false;
    let mut quote: Option<char> = None;
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some('"'), '\\') => cur.extend(chars.next()),
            (Some(_), c) => cur.push(c),
            (None, '"' | '\'') => {
                quote = Some(c);
                in_word = true;
            }
            (None, c) if c.is_whitespace() => {
                if in_word {
                    words.push(std::mem::take(&mut cur));
                    in_word = false;
                }
            }
            (None, c) => {
                cur.push(c);
                in_word = true;
            }
        }
    }
    if quote.is_some() {
        bail!("unterminated quote");
    }
    if in_word {
        words.push(cur);
    }
    Ok(words)
}

/// `/help` text: all modules, or one module's commands.
pub fn help(registry: &Registry, module: Option<&str>) -> String {
    match module.and_then(|m| registry.find_module(m)) {
        Some(m) => {
            let mut out = format!("{} — {}\n", m.title, m.tagline);
            for s in registry.module_actions(m.id) {
                let params: Vec<String> =
                    s.params.iter().map(|p| if p.required { format!("<{}>", p.name) } else { format!("[{}=…]", p.name) }).collect();
                out.push_str(&format!("\n/{} {} {}\n    {}", m.id, s.verb().replace('_', "-"), params.join(" "), s.description));
            }
            out
        }
        None => {
            let mut out = String::from("Type a request in plain language, or a command:\n");
            for m in &registry.modules {
                out.push_str(&format!("\n/help {:<12} {}", m.id, m.tagline));
            }
            out
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{validate, ActionSpec, Output, Param, Risk};

    const OPEN: ActionSpec = ActionSpec {
        id: "firewall.open_port",
        title: "Open port",
        description: "test",
        risk: Risk::Change,
        params: &[
            Param::text("port", "port").check(validate::port),
            Param::choice("protocol", &["tcp", "udp"], "proto").default("tcp"),
            Param::text("zone", "zone").optional(),
        ],
    };

    fn registry() -> Registry {
        let mut reg = Registry::default();
        reg.action(OPEN, |_, _| async { Ok(Output::Done(String::new())) });
        reg
    }

    fn parse_args(line: &str) -> Args {
        match parse(&registry(), line).expect("parses") {
            Parsed::Action { spec, raw } => spec.validate(&raw).expect("valid"),
            Parsed::Help(_) => panic!("expected an action"),
        }
    }

    #[test]
    fn positional_and_named_arguments() {
        let a = parse_args("/firewall open-port 8080 udp");
        assert_eq!((a.str("port"), a.str("protocol")), ("8080", "udp"));
        let a = parse_args("/firewall open_port zone=public 8080");
        assert_eq!((a.str("port"), a.str("protocol"), a.str("zone")), ("8080", "tcp", "public"));
    }

    #[test]
    fn format_round_trips() {
        // Optional parameters are always spelled out, so the formatted command
        // is canonical however it was typed.
        for (typed, canonical) in [
            ("/firewall open-port 8080", "/firewall open-port 8080"),
            ("/firewall open-port 8080 udp", "/firewall open-port 8080 protocol=udp"),
            ("/firewall open-port zone=\"my zone\" 8080", "/firewall open-port 8080 zone=\"my zone\""),
        ] {
            let args = parse_args(typed);
            assert_eq!(format(&OPEN, &args), canonical);
            assert_eq!(parse_args(canonical), args);
        }
    }

    #[test]
    fn rejects_bad_input() {
        let reg = registry();
        assert!(parse(&reg, "/firewall nope").is_err());
        assert!(parse(&reg, "/firewall open-port \"unterminated").is_err());
        assert!(parse(&reg, "/firewall open-port 1 tcp zone extra").is_err());
        let Ok(Parsed::Action { spec, raw }) = parse(&reg, "/firewall open-port 99999") else { panic!() };
        assert!(spec.validate(&raw).is_err());
        let Ok(Parsed::Action { spec, raw }) = parse(&reg, "/firewall open-port 80 icmp") else { panic!() };
        assert!(spec.validate(&raw).is_err());
    }

    #[test]
    fn unknown_parameters_are_refused() {
        let mut raw = serde_json::Map::new();
        raw.insert("port".into(), "80".into());
        raw.insert("command".into(), "rm -rf /".into());
        assert!(OPEN.validate(&raw).is_err());
    }

    #[test]
    fn help_lists_modules() {
        assert!(matches!(parse(&registry(), "/help").unwrap(), Parsed::Help(None)));
    }
}
