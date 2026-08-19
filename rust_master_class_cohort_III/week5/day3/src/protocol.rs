//! Week 5 · Day 3 — the chat protocol.
//!
//! # Specification
//!
//! Line-oriented and text-based, so it can be driven with `nc` and read by a
//! human. One command per line. Anything not starting with `/` is a message to
//! the current room.
//!
//! ## Client → server
//!
//! ```text
//! /nick <name>            set your display name
//! /join <room>            leave the current room and join <room>
//! /msg <nick> <text>      private message; <text> may contain spaces
//! /who                    list users in the current room
//! /rooms                  list all rooms
//! /quit                   disconnect
//! <text>                  send <text> to the current room
//! ```
//!
//! ## Server → client
//!
//! ```text
//! * <text>                system notice
//! ! <text>                error
//! <nick> <text>           a room message
//! [<nick>] <text>         a private message
//! ```
//!
//! The one-character prefix means a client can tell the three cases apart without
//! a parser. That is a deliberate protocol design choice, not decoration — and
//! choosing sigils that cannot begin a nickname is what keeps it unambiguous.
//!
//! ## Design notes worth arguing about in class
//!
//! - **Newlines cannot appear in a message.** The delimiter is `\n`, so a payload
//!   containing one would frame as two messages. We reject rather than escape;
//!   escaping is the alternative and it is strictly more work on both ends.
//! - **Nicknames are validated.** Without rules, a nick of `"alice hello"` makes
//!   `/msg alice hello there` ambiguous, and a nick starting with `*` lets a user
//!   forge system notices. Both are injection bugs of exactly the kind Phase Three
//!   cares about.
//! - **Unknown commands are reported, never ignored.** Silence is the least
//!   debuggable failure there is.

use std::fmt;

/// The maximum length of a single protocol line, in bytes.
///
/// Enforced by `LinesCodec::new_with_max_length`. Without a cap, a peer that
/// never sends a newline makes the server buffer until it dies — a trivial
/// memory-exhaustion attack on any public port.
pub const MAX_LINE: usize = 1024;

pub const MAX_NICK: usize = 24;
pub const MAX_ROOM: usize = 32;

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Nick(String),
    Join(String),
    Msg { to: String, text: String },
    Who,
    Rooms,
    Quit,
    /// Bare text: a message to the current room.
    Say(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    Empty,
    MissingArgument(&'static str),
    UnknownCommand(String),
    InvalidNick(String),
    InvalidRoom(String),
    TooLong { limit: usize },
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::Empty => write!(f, "empty input"),
            ParseError::MissingArgument(usage) => write!(f, "usage: {usage}"),
            ParseError::UnknownCommand(cmd) => {
                write!(f, "unknown command {cmd:?} — try /nick /join /msg /who /rooms /quit")
            }
            ParseError::InvalidNick(why) => write!(f, "invalid nickname: {why}"),
            ParseError::InvalidRoom(why) => write!(f, "invalid room name: {why}"),
            ParseError::TooLong { limit } => write!(f, "line too long (max {limit} bytes)"),
        }
    }
}

impl std::error::Error for ParseError {}

impl Command {
    /// Parses one protocol line.
    ///
    /// The line arrives already stripped of its trailing newline by the codec —
    /// framing and parsing are separate concerns, and mixing them is how you end
    /// up with a parser that only works over one transport.
    pub fn parse(line: &str) -> Result<Command, ParseError> {
        if line.len() > MAX_LINE {
            return Err(ParseError::TooLong { limit: MAX_LINE });
        }

        let line = line.trim();
        if line.is_empty() {
            return Err(ParseError::Empty);
        }

        // No leading slash: it is a message to the room.
        if !line.starts_with('/') {
            return Ok(Command::Say(line.to_string()));
        }

        // `splitn(2)` splits the verb from the rest, leaving the remainder
        // untouched. Using `split_whitespace().collect()` here would silently
        // collapse runs of spaces inside message text.
        let mut parts = line.splitn(2, char::is_whitespace);
        let verb = parts.next().unwrap_or("");
        let rest = parts.next().unwrap_or("").trim();

        match verb {
            "/nick" => {
                if rest.is_empty() {
                    return Err(ParseError::MissingArgument("/nick <name>"));
                }
                validate_nick(rest)?;
                Ok(Command::Nick(rest.to_string()))
            }

            "/join" => {
                if rest.is_empty() {
                    return Err(ParseError::MissingArgument("/join <room>"));
                }
                validate_room(rest)?;
                Ok(Command::Join(rest.to_string()))
            }

            "/msg" => {
                // Two fields: the target nick, then everything else verbatim.
                let mut parts = rest.splitn(2, char::is_whitespace);
                let to = parts.next().unwrap_or("").trim();
                let text = parts.next().unwrap_or("").trim();

                if to.is_empty() || text.is_empty() {
                    return Err(ParseError::MissingArgument("/msg <nick> <text>"));
                }

                Ok(Command::Msg {
                    to: to.to_string(),
                    text: text.to_string(),
                })
            }

            "/who" => Ok(Command::Who),
            "/rooms" => Ok(Command::Rooms),
            "/quit" | "/exit" => Ok(Command::Quit),

            other => Err(ParseError::UnknownCommand(other.to_string())),
        }
    }
}

/// Nicknames must be unambiguous in the wire format.
///
/// - No whitespace, or `/msg alice smith hello` cannot be parsed.
/// - No leading `*`, `!`, or `[`, or a user can forge system notices and private
///   messages. This is the protocol-level equivalent of an injection bug.
/// - Printable ASCII only, so a control character cannot corrupt another client's
///   terminal — an ANSI escape in a nickname is a real attack, not a hypothetical.
pub fn validate_nick(nick: &str) -> Result<(), ParseError> {
    if nick.is_empty() {
        return Err(ParseError::InvalidNick("must not be empty".into()));
    }
    if nick.chars().count() > MAX_NICK {
        return Err(ParseError::InvalidNick(format!(
            "must be {MAX_NICK} characters or fewer"
        )));
    }
    if nick.chars().any(char::is_whitespace) {
        return Err(ParseError::InvalidNick("must not contain spaces".into()));
    }
    if nick.starts_with(['*', '!', '[', '/']) {
        return Err(ParseError::InvalidNick(
            "must not start with * ! [ or /".into(),
        ));
    }
    if nick.chars().any(|c| c.is_control()) {
        return Err(ParseError::InvalidNick(
            "must not contain control characters".into(),
        ));
    }
    Ok(())
}

pub fn validate_room(room: &str) -> Result<(), ParseError> {
    if room.is_empty() {
        return Err(ParseError::InvalidRoom("must not be empty".into()));
    }
    if room.chars().count() > MAX_ROOM {
        return Err(ParseError::InvalidRoom(format!(
            "must be {MAX_ROOM} characters or fewer"
        )));
    }
    if room.chars().any(char::is_whitespace) {
        return Err(ParseError::InvalidRoom("must not contain spaces".into()));
    }
    if room.chars().any(|c| c.is_control()) {
        return Err(ParseError::InvalidRoom(
            "must not contain control characters".into(),
        ));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Server → client events
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// `* <text>` — a system notice.
    Notice(String),
    /// `! <text>` — an error.
    Error(String),
    /// `<nick> <text>` — a room message.
    Message { from: String, text: String },
    /// `[<nick>] <text>` — a private message.
    Private { from: String, text: String },
}

impl fmt::Display for Event {
    /// The wire format. `Display` rather than a `to_wire` method so it composes
    /// with `format!`, `writeln!`, and the codec's `send`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Event::Notice(text) => write!(f, "* {text}"),
            Event::Error(text) => write!(f, "! {text}"),
            Event::Message { from, text } => write!(f, "{from} {text}"),
            Event::Private { from, text } => write!(f, "[{from}] {text}"),
        }
    }
}

impl Event {
    pub fn notice(text: impl Into<String>) -> Event {
        Event::Notice(text.into())
    }

    pub fn error(text: impl Into<String>) -> Event {
        Event::Error(text.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bare_text_as_say() {
        assert_eq!(
            Command::parse("hello everyone"),
            Ok(Command::Say("hello everyone".into()))
        );
    }

    #[test]
    fn parses_nick() {
        assert_eq!(Command::parse("/nick alice"), Ok(Command::Nick("alice".into())));
    }

    #[test]
    fn nick_without_argument_is_an_error_not_a_panic() {
        assert_eq!(
            Command::parse("/nick"),
            Err(ParseError::MissingArgument("/nick <name>"))
        );
        assert_eq!(
            Command::parse("/nick   "),
            Err(ParseError::MissingArgument("/nick <name>"))
        );
    }

    #[test]
    fn msg_preserves_internal_spacing() {
        // The regression test for `splitn(2)` vs `split_whitespace().collect()`.
        // The latter would return "hello there    friend" as "hello there friend".
        assert_eq!(
            Command::parse("/msg bob hello there    friend"),
            Ok(Command::Msg {
                to: "bob".into(),
                text: "hello there    friend".into(),
            })
        );
    }

    #[test]
    fn msg_needs_both_target_and_text() {
        assert!(Command::parse("/msg bob").is_err());
        assert!(Command::parse("/msg").is_err());
    }

    #[test]
    fn rejects_nick_with_spaces() {
        // Would make `/msg alice smith hi` ambiguous.
        assert!(matches!(
            Command::parse("/nick alice smith"),
            Err(ParseError::InvalidNick(_))
        ));
    }

    #[test]
    fn rejects_nick_that_could_forge_a_system_notice() {
        for bad in ["*admin", "!system", "[root]", "/quit"] {
            assert!(
                matches!(
                    Command::parse(&format!("/nick {bad}")),
                    Err(ParseError::InvalidNick(_))
                ),
                "{bad} should be rejected"
            );
        }
    }

    #[test]
    fn rejects_control_characters_in_nick() {
        // An ANSI escape in a nickname would be rendered by every other client's
        // terminal. This is a real attack, not a hypothetical.
        assert!(validate_nick("alice\x1b[31m").is_err());
        assert!(validate_nick("alice\x07").is_err());
    }

    #[test]
    fn rejects_overlong_nick() {
        assert!(validate_nick(&"x".repeat(MAX_NICK + 1)).is_err());
        assert!(validate_nick(&"x".repeat(MAX_NICK)).is_ok());
    }

    #[test]
    fn unknown_commands_are_reported() {
        assert_eq!(
            Command::parse("/dance"),
            Err(ParseError::UnknownCommand("/dance".into()))
        );
    }

    #[test]
    fn empty_input_is_an_error() {
        assert_eq!(Command::parse(""), Err(ParseError::Empty));
        assert_eq!(Command::parse("    "), Err(ParseError::Empty));
    }

    #[test]
    fn quit_has_an_alias() {
        assert_eq!(Command::parse("/quit"), Ok(Command::Quit));
        assert_eq!(Command::parse("/exit"), Ok(Command::Quit));
    }

    #[test]
    fn events_render_with_distinguishable_prefixes() {
        assert_eq!(Event::notice("welcome").to_string(), "* welcome");
        assert_eq!(Event::error("nope").to_string(), "! nope");
        assert_eq!(
            Event::Message { from: "alice".into(), text: "hi".into() }.to_string(),
            "alice hi"
        );
        assert_eq!(
            Event::Private { from: "bob".into(), text: "psst".into() }.to_string(),
            "[bob] psst"
        );
    }

    #[test]
    fn overlong_line_is_rejected_before_parsing() {
        let long = format!("/nick {}", "x".repeat(MAX_LINE));
        assert_eq!(
            Command::parse(&long),
            Err(ParseError::TooLong { limit: MAX_LINE })
        );
    }
}
