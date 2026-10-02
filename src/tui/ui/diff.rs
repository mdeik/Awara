use super::*;

pub fn diff_strings(orig: &str, new: &str) -> (Vec<Span<'static>>, Vec<Span<'static>>) {
    let orig_chars: Vec<char> = orig.chars().collect();
    let new_chars: Vec<char> = new.chars().collect();
    let m = orig_chars.len();
    let n = new_chars.len();

    // LCS DP table
    let mut dp = vec![vec![0; n + 1]; m + 1];
    for i in 1..=m {
        for j in 1..=n {
            if orig_chars[i - 1] == new_chars[j - 1] {
                dp[i][j] = dp[i - 1][j - 1] + 1;
            } else {
                dp[i][j] = std::cmp::max(dp[i - 1][j], dp[i][j - 1]);
            }
        }
    }

    // Backtrack to find diff
    let mut i = m;
    let mut j = n;

    #[derive(Debug)]
    enum Op {
        Keep(char),
        Delete(char),
        Insert(char),
    }
    let mut ops = Vec::new();

    while i > 0 && j > 0 {
        if orig_chars[i - 1] == new_chars[j - 1] {
            ops.push(Op::Keep(orig_chars[i - 1]));
            i -= 1;
            j -= 1;
        } else if dp[i - 1][j] >= dp[i][j - 1] {
            ops.push(Op::Delete(orig_chars[i - 1]));
            i -= 1;
        } else {
            ops.push(Op::Insert(new_chars[j - 1]));
            j -= 1;
        }
    }

    while i > 0 {
        ops.push(Op::Delete(orig_chars[i - 1]));
        i -= 1;
    }
    while j > 0 {
        ops.push(Op::Insert(new_chars[j - 1]));
        j -= 1;
    }

    ops.reverse();

    // Construct Spans
    let mut orig_spans = Vec::new();
    let mut new_spans = Vec::new();

    let mut current_orig_text = String::new();
    let mut current_orig_style = Style::default();

    let mut current_new_text = String::new();
    let mut current_new_style = Style::default();

    for op in ops {
        match op {
            Op::Keep(c) => {
                if current_orig_style != Style::default() {
                    if !current_orig_text.is_empty() {
                        orig_spans
                            .push(Span::styled(current_orig_text.clone(), current_orig_style));
                        current_orig_text.clear();
                    }
                    current_orig_style = Style::default();
                }
                current_orig_text.push(c);

                if current_new_style != Style::default() {
                    if !current_new_text.is_empty() {
                        new_spans.push(Span::styled(current_new_text.clone(), current_new_style));
                        current_new_text.clear();
                    }
                    current_new_style = Style::default();
                }
                current_new_text.push(c);
            }
            Op::Delete(c) => {
                if current_orig_style != Style::default().fg(Color::Red) {
                    if !current_orig_text.is_empty() {
                        orig_spans
                            .push(Span::styled(current_orig_text.clone(), current_orig_style));
                        current_orig_text.clear();
                    }
                    current_orig_style = Style::default().fg(Color::Red);
                }
                current_orig_text.push(c);
                // Nothing for new
            }
            Op::Insert(c) => {
                if current_new_style != Style::default().fg(Color::Green) {
                    if !current_new_text.is_empty() {
                        new_spans.push(Span::styled(current_new_text.clone(), current_new_style));
                        current_new_text.clear();
                    }
                    current_new_style = Style::default().fg(Color::Green);
                }
                current_new_text.push(c);
                // Nothing for orig
            }
        }
    }

    // Final flush
    if !current_orig_text.is_empty() {
        orig_spans.push(Span::styled(current_orig_text, current_orig_style));
    }
    if !current_new_text.is_empty() {
        new_spans.push(Span::styled(current_new_text, current_new_style));
    }

    (orig_spans, new_spans)
}

pub(crate) fn human_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * 1024;
    const GB: u64 = 1024 * 1024 * 1024;
    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}
