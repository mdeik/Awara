use super::*;
use ratatui::style::{Color, Style};

#[test]
fn test_diff_strings_coloring() {
    let (orig, new) = diff_strings("foo", "foobar");
    // orig: "foo" (default style)
    // new: "foo" (default), "bar" (green)

    assert_eq!(orig.len(), 1);
    assert_eq!(orig[0].content, "foo");
    assert_eq!(orig[0].style, Style::default());

    assert_eq!(new.len(), 2);
    assert_eq!(new[0].content, "foo");
    assert_eq!(new[0].style, Style::default());
    assert_eq!(new[1].content, "bar");
    assert_eq!(new[1].style, Style::default().fg(Color::Green));
}
