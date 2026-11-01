// Pure live-typing logic: local agreement, held-back word, reconcile diff and
// per-segment character accounting. Text here is fixed test data.

use deepai_voice_lib::dictation::stream_text::{
    Edit, SegmentTyping, cluster_count, reconcile, stable_text,
};

fn edit(backspaces: usize, text: &str) -> Edit {
    Edit {
        backspaces,
        text: text.into(),
    }
}

#[test]
fn clusters_count_emoji_and_accents_as_one_key_press() {
    assert_eq!(cluster_count(""), 0);
    assert_eq!(cluster_count("abc"), 3);
    assert_eq!(cluster_count("café"), 4);
    // e + combining acute accent.
    assert_eq!(cluster_count("cafe\u{301}"), 4);
    assert_eq!(cluster_count("👍🏽"), 1);
    assert_eq!(cluster_count("👨\u{200d}👩\u{200d}👧"), 1);
    assert_eq!(cluster_count("❤️ ok"), 4);
}

#[test]
fn stable_text_keeps_agreed_words_and_holds_back_the_last_one() {
    assert_eq!(stable_text("", "hello world"), "");
    assert_eq!(stable_text("hello world", ""), "");
    assert_eq!(stable_text("hello world", "hello world how"), "hello world");
    // The newest last word is always held back, even when both agree.
    assert_eq!(stable_text("hello world", "hello world"), "hello");
    // Disagreement stops the run, punctuation counts.
    assert_eq!(stable_text("hello world how", "hello, world how are"), "");
    assert_eq!(stable_text("we went to", "we want to go"), "we");
    // Repeated words are compared by position.
    assert_eq!(stable_text("no no no", "no no no no"), "no no no");
    // A shrinking hypothesis caps the result.
    assert_eq!(stable_text("one two three four", "one two"), "one");
    // Whitespace is normalised.
    assert_eq!(stable_text(" a   b  c ", "a b c d"), "a b c");
}

#[test]
fn reconcile_types_only_the_remainder_or_backspaces_the_mismatch() {
    assert_eq!(reconcile("", "hello"), edit(0, "hello"));
    assert_eq!(reconcile("hello", "hello world"), edit(0, " world"));
    assert_eq!(reconcile("hello world", "hello world"), edit(0, ""));
    assert_eq!(reconcile("hello wold", "hello world."), edit(2, "rld."));
    assert_eq!(reconcile("hello", ""), edit(5, ""));
    assert_eq!(reconcile("Hello", "hello"), edit(5, "hello"));
    // Emoji and accents: backspace counts whole clusters, never splits one.
    assert_eq!(reconcile("ok 👍🏽", "ok 👍"), edit(1, "👍"));
    assert_eq!(reconcile("café", "cafe"), edit(1, "e"));
    assert_eq!(reconcile("naïve x", "naïve y"), edit(1, "y"));
}

#[test]
fn segment_types_agreed_words_as_hypotheses_grow() {
    let mut s = SegmentTyping::new(false);
    assert_eq!(s.on_hypothesis("hello world"), None);
    assert_eq!(s.preview("hello world"), "hello world");
    assert_eq!(
        s.on_hypothesis("hello world how"),
        Some("hello world".to_string())
    );
    assert_eq!(s.typed(), "hello world");
    assert_eq!(s.preview("hello world how"), "how");
    assert_eq!(
        s.on_hypothesis("hello world how are you"),
        Some(" how".to_string())
    );
    // A hypothesis that disagrees with typed text types nothing more.
    assert_eq!(s.on_hypothesis("yellow world how are you"), None);
    assert_eq!(s.on_hypothesis("yellow world how are you today"), None);
    assert_eq!(s.typed(), "hello world how");
    // Empty and shrinking hypotheses never type or delete.
    assert_eq!(s.on_hypothesis(""), None);
    assert_eq!(s.on_hypothesis("hello"), None);
    assert_eq!(s.typed(), "hello world how");
}

#[test]
fn segment_final_reconciles_within_this_segment_only() {
    let mut s = SegmentTyping::new(false);
    s.on_hypothesis("hello world how");
    s.on_hypothesis("hello world how are");
    assert_eq!(s.typed(), "hello world how");
    assert_eq!(
        s.on_final(Some("Hello world, how are you.")),
        edit(15, "Hello world, how are you.")
    );
    assert_eq!(s.typed(), "Hello world, how are you.");

    let mut s = SegmentTyping::new(false);
    s.on_hypothesis("one two three");
    s.on_hypothesis("one two three four");
    assert_eq!(s.on_final(Some("one two three four.")), edit(0, " four."));
    // Final with nothing typed yet types everything.
    let mut s = SegmentTyping::new(false);
    assert_eq!(s.on_final(Some("hi")), edit(0, "hi"));
    // Final that filtered to nothing removes what this segment typed.
    let mut s = SegmentTyping::new(true);
    s.on_hypothesis("thank you so");
    s.on_hypothesis("thank you so much");
    assert_eq!(s.typed(), " thank you so");
    assert_eq!(s.on_final(None), edit(13, ""));
    assert_eq!(s.typed(), "");
}

#[test]
fn later_segments_start_with_one_space() {
    let mut s = SegmentTyping::new(true);
    s.on_hypothesis("next one");
    assert_eq!(s.on_hypothesis("next one two"), Some(" next one".into()));
    assert_eq!(s.on_final(Some("Next one two.")), edit(8, "Next one two."));
    assert_eq!(s.typed(), " Next one two.");
    let mut s = SegmentTyping::new(true);
    assert_eq!(s.on_final(Some("x")), edit(0, " x"));
    let mut s = SegmentTyping::new(true);
    assert_eq!(s.on_final(None), edit(0, ""));
}

#[test]
fn backspaces_never_exceed_what_this_segment_typed() {
    let cases = [
        ("a b c", "a b c d", Some("zzz")),
        ("👍🏽 x", "👍🏽 x y", Some("👍 x y")),
        ("same same", "same same same", None),
    ];
    for lead in [false, true] {
        for (h1, h2, fin) in cases {
            let mut s = SegmentTyping::new(lead);
            s.on_hypothesis(h1);
            s.on_hypothesis(h2);
            let typed = cluster_count(s.typed());
            let e = s.on_final(fin);
            assert!(e.backspaces <= typed, "{h1:?} {h2:?} {fin:?}");
        }
    }
}
