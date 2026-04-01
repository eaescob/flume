use std::collections::HashMap;
use std::sync::OnceLock;

/// Get the emoji shortcode map.
fn emoji_map() -> &'static HashMap<&'static str, &'static str> {
    static MAP: OnceLock<HashMap<&str, &str>> = OnceLock::new();
    MAP.get_or_init(|| {
        let mut m = HashMap::new();
        // Smileys & People
        m.insert("smile", "😄");
        m.insert("grin", "😁");
        m.insert("joy", "😂");
        m.insert("rofl", "🤣");
        m.insert("smiley", "😃");
        m.insert("wink", "😉");
        m.insert("blush", "😊");
        m.insert("heart_eyes", "😍");
        m.insert("kissing_heart", "😘");
        m.insert("thinking", "🤔");
        m.insert("shush", "🤫");
        m.insert("zipper_mouth", "🤐");
        m.insert("raised_eyebrow", "🤨");
        m.insert("neutral", "😐");
        m.insert("expressionless", "😑");
        m.insert("unamused", "😒");
        m.insert("rolling_eyes", "🙄");
        m.insert("grimacing", "😬");
        m.insert("relieved", "😌");
        m.insert("pensive", "😔");
        m.insert("sleepy", "😪");
        m.insert("sleeping", "😴");
        m.insert("mask", "😷");
        m.insert("nerd", "🤓");
        m.insert("sunglasses", "😎");
        m.insert("confused", "😕");
        m.insert("worried", "😟");
        m.insert("frown", "☹️");
        m.insert("open_mouth", "😮");
        m.insert("hushed", "😯");
        m.insert("astonished", "😲");
        m.insert("flushed", "😳");
        m.insert("scream", "😱");
        m.insert("cry", "😢");
        m.insert("sob", "😭");
        m.insert("angry", "😠");
        m.insert("rage", "🤬");
        m.insert("skull", "💀");
        m.insert("poop", "💩");
        m.insert("clown", "🤡");
        m.insert("ghost", "👻");
        m.insert("alien", "👽");
        m.insert("robot", "🤖");
        m.insert("wave", "👋");
        m.insert("ok_hand", "👌");
        m.insert("pinched", "🤌");
        m.insert("v", "✌️");
        m.insert("crossed_fingers", "🤞");
        m.insert("metal", "🤘");
        m.insert("call_me", "🤙");
        m.insert("point_left", "👈");
        m.insert("point_right", "👉");
        m.insert("point_up", "👆");
        m.insert("point_down", "👇");
        m.insert("thumbsup", "👍");
        m.insert("thumbsdown", "👎");
        m.insert("+1", "👍");
        m.insert("-1", "👎");
        m.insert("fist", "✊");
        m.insert("punch", "👊");
        m.insert("clap", "👏");
        m.insert("pray", "🙏");
        m.insert("handshake", "🤝");
        m.insert("muscle", "💪");
        m.insert("brain", "🧠");
        m.insert("eyes", "👀");
        m.insert("eye", "👁️");
        m.insert("salute", "🫡");
        // Hearts & Symbols
        m.insert("heart", "❤️");
        m.insert("orange_heart", "🧡");
        m.insert("yellow_heart", "💛");
        m.insert("green_heart", "💚");
        m.insert("blue_heart", "💙");
        m.insert("purple_heart", "💜");
        m.insert("black_heart", "🖤");
        m.insert("broken_heart", "💔");
        m.insert("fire", "🔥");
        m.insert("star", "⭐");
        m.insert("sparkles", "✨");
        m.insert("zap", "⚡");
        m.insert("boom", "💥");
        m.insert("100", "💯");
        m.insert("check", "✅");
        m.insert("x", "❌");
        m.insert("warning", "⚠️");
        m.insert("question", "❓");
        m.insert("exclamation", "❗");
        m.insert("pin", "📌");
        m.insert("lock", "🔒");
        m.insert("unlock", "🔓");
        m.insert("key", "🔑");
        m.insert("bell", "🔔");
        m.insert("mega", "📣");
        m.insert("mute", "🔇");
        // Objects & Tech
        m.insert("computer", "💻");
        m.insert("keyboard", "⌨️");
        m.insert("phone", "📱");
        m.insert("globe", "🌍");
        m.insert("link", "🔗");
        m.insert("bulb", "💡");
        m.insert("gear", "⚙️");
        m.insert("wrench", "🔧");
        m.insert("hammer", "🔨");
        m.insert("shield", "🛡️");
        m.insert("bug", "🐛");
        m.insert("rocket", "🚀");
        m.insert("package", "📦");
        m.insert("memo", "📝");
        m.insert("book", "📖");
        m.insert("clipboard", "📋");
        m.insert("calendar", "📅");
        m.insert("chart", "📈");
        m.insert("chart_down", "📉");
        m.insert("email", "📧");
        m.insert("inbox", "📥");
        m.insert("outbox", "📤");
        // Food & Drink
        m.insert("coffee", "☕");
        m.insert("beer", "🍺");
        m.insert("beers", "🍻");
        m.insert("wine", "🍷");
        m.insert("cocktail", "🍸");
        m.insert("pizza", "🍕");
        m.insert("taco", "🌮");
        m.insert("cookie", "🍪");
        m.insert("cake", "🎂");
        // Nature & Weather
        m.insert("sun", "☀️");
        m.insert("cloud", "☁️");
        m.insert("rain", "🌧️");
        m.insert("snow", "❄️");
        m.insert("rainbow", "🌈");
        m.insert("tree", "🌲");
        m.insert("flower", "🌸");
        m.insert("cactus", "🌵");
        // Animals
        m.insert("cat", "🐱");
        m.insert("dog", "🐶");
        m.insert("fox", "🦊");
        m.insert("bear", "🐻");
        m.insert("panda", "🐼");
        m.insert("penguin", "🐧");
        m.insert("owl", "🦉");
        m.insert("unicorn", "🦄");
        m.insert("snake", "🐍");
        m.insert("crab", "🦀");
        m.insert("shrimp", "🦐");
        // Activities & Celebrations
        m.insert("party", "🎉");
        m.insert("tada", "🎉");
        m.insert("balloon", "🎈");
        m.insert("confetti", "🎊");
        m.insert("trophy", "🏆");
        m.insert("medal", "🏅");
        m.insert("crown", "👑");
        m.insert("gift", "🎁");
        m.insert("gaming", "🎮");
        m.insert("dice", "🎲");
        m.insert("music", "🎵");
        m.insert("headphones", "🎧");
        m.insert("guitar", "🎸");
        // Flags & misc
        m.insert("flag_us", "🇺🇸");
        m.insert("flag_uk", "🇬🇧");
        m.insert("flag_de", "🇩🇪");
        m.insert("flag_fr", "🇫🇷");
        m.insert("flag_jp", "🇯🇵");
        m.insert("pirate", "🏴‍☠️");
        m
    })
}

/// Replace all `:shortcode:` patterns in text with their emoji.
pub fn replace_shortcodes(text: &str) -> String {
    let map = emoji_map();
    let mut result = String::with_capacity(text.len());
    let mut chars = text.char_indices().peekable();

    while let Some((i, ch)) = chars.next() {
        if ch == ':' {
            // Look for closing ':'
            let rest = &text[i + 1..];
            if let Some(end) = rest.find(':') {
                let code = &rest[..end];
                // Valid shortcode: alphanumeric, underscore, plus, minus
                if !code.is_empty()
                    && code.len() <= 30
                    && code.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '+' || c == '-')
                {
                    if let Some(emoji) = map.get(code) {
                        result.push_str(emoji);
                        // Advance past the closing ':'
                        for _ in 0..=end {
                            chars.next();
                        }
                        continue;
                    }
                }
            }
        }
        result.push(ch);
    }

    result
}

/// Find shortcode completions matching a prefix.
/// Returns up to `limit` matches sorted alphabetically.
pub fn complete_shortcode(prefix: &str) -> Vec<(&'static str, &'static str)> {
    let map = emoji_map();
    let prefix_lower = prefix.to_lowercase();
    let mut matches: Vec<(&str, &str)> = map
        .iter()
        .filter(|(k, _)| k.starts_with(&prefix_lower))
        .map(|(k, v)| (*k, *v))
        .collect();
    matches.sort_by_key(|(k, _)| *k);
    matches.truncate(10);
    matches
}

/// Get all shortcode names (for help/listing).
pub fn shortcode_count() -> usize {
    emoji_map().len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replace_basic() {
        assert_eq!(replace_shortcodes("hello :wave:"), "hello 👋");
        assert_eq!(replace_shortcodes(":fire: hot :fire:"), "🔥 hot 🔥");
        assert_eq!(replace_shortcodes(":thumbsup:"), "👍");
    }

    #[test]
    fn replace_unknown_shortcode() {
        assert_eq!(replace_shortcodes(":notanemoji:"), ":notanemoji:");
    }

    #[test]
    fn replace_no_shortcodes() {
        assert_eq!(replace_shortcodes("just text"), "just text");
        assert_eq!(replace_shortcodes("has : colons : but not codes"), "has : colons : but not codes");
    }

    #[test]
    fn replace_mixed() {
        assert_eq!(
            replace_shortcodes("hey :wave: how are you :smile:?"),
            "hey 👋 how are you 😄?"
        );
    }

    #[test]
    fn replace_adjacent() {
        assert_eq!(replace_shortcodes(":heart::heart:"), "❤️❤️");
    }

    #[test]
    fn complete_prefix() {
        let matches = complete_shortcode("thumb");
        assert!(matches.iter().any(|(k, _)| *k == "thumbsup"));
        assert!(matches.iter().any(|(k, _)| *k == "thumbsdown"));
    }

    #[test]
    fn complete_empty_prefix() {
        let matches = complete_shortcode("zzz_nothing");
        assert!(matches.is_empty());
    }

    #[test]
    fn shortcode_count_reasonable() {
        assert!(shortcode_count() > 150);
    }
}
