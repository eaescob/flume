/// Build an XDCC SEND request message.
/// Sends: `xdcc send #<pack_number>` as a PRIVMSG to the bot.
pub fn request_pack(pack_number: u32) -> String {
    format!("xdcc send #{}", pack_number)
}

/// Build an XDCC LIST request message.
pub fn request_list() -> String {
    "xdcc list".to_string()
}

/// Build an XDCC CANCEL request message.
pub fn request_cancel() -> String {
    "xdcc cancel".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xdcc_request_pack() {
        assert_eq!(request_pack(42), "xdcc send #42");
        assert_eq!(request_pack(1), "xdcc send #1");
    }

    #[test]
    fn xdcc_request_list() {
        assert_eq!(request_list(), "xdcc list");
    }

    #[test]
    fn xdcc_cancel() {
        assert_eq!(request_cancel(), "xdcc cancel");
    }
}
