//! Chinese word segmentation for keyword search (B3-07). `unicode61` cannot split CJK text, so
//! the index holds text already cut into words by jieba and queries are cut the same way.

use std::sync::OnceLock;

use jieba_rs::Jieba;
use memory_core::Result;
use rusqlite::{Connection, OptionalExtension, params};

use crate::storage;

/// Bump when the segmentation or the dictionary changes: the index is rebuilt on next open.
pub(crate) const SEGMENT_VERSION: &str = "jieba-rs-0.11-default-dict-1";

pub(crate) fn is_cjk(c: char) -> bool {
    matches!(c,
        '\u{3040}'..='\u{30FF}' | '\u{3400}'..='\u{4DBF}' | '\u{4E00}'..='\u{9FFF}'
        | '\u{AC00}'..='\u{D7AF}' | '\u{F900}'..='\u{FAFF}')
}

pub(crate) fn has_cjk(s: &str) -> bool {
    s.chars().any(is_cjk)
}

/// The dictionary takes a while to load, so it is built on first use and only for CJK text.
fn jieba() -> &'static Jieba {
    static J: OnceLock<Jieba> = OnceLock::new();
    J.get_or_init(Jieba::new)
}

/// Particles and question words that match almost every Chinese sentence. Negations (不, 没,
/// 不能, 禁止) are deliberately absent: they change the meaning. Applied to queries only, so
/// changing the list never needs a reindex.
const STOP_WORDS: &str = "\
    的 了 吗 呢 啊 吧 呀 嘛 哦 么 是 在 和 与 或 也 都 就 还 把 被 让 给 对 从 到 这 那 这个 那个 \
    我 我们 你 你们 它 他 她 什么 怎么 怎样 怎么样 为什么 为何 如何 哪 哪里 哪个 有没有 是否 \
    会 要 可以 一个 一下 一些 有 能 通过 别的 其他 某 某条 里 中 出 问题 注意 时候 应该 需要 \
    进行 使用 用 因为 所以 如果 但是 而且 以及 并且 然后 已经 很";

/// Index side: finest-grained words (`cut_for_search`) joined by spaces. Text without CJK is
/// returned unchanged, so identifiers like `busy_timeout` stay whole.
pub(crate) fn for_index(text: &str) -> String {
    if !has_cjk(text) {
        return text.to_string();
    }
    let words: Vec<&str> = jieba()
        .cut_for_search(text, true)
        .into_iter()
        .map(|t| t.word)
        .collect();
    words.join(" ")
}

/// Query side: words without whitespace, punctuation and stop words, in order, duplicates
/// removed. Latin words of any length are kept.
pub(crate) fn for_query(query: &str) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    jieba()
        .cut(query, true)
        .into_iter()
        .map(|t| t.word.trim())
        .filter(|w| w.chars().any(|c| c.is_alphanumeric()))
        .filter(|w| !STOP_WORDS.split_whitespace().any(|s| s == *w))
        .map(str::to_lowercase)
        .filter(|w| seen.insert(w.clone()))
        .collect()
}

/// Terms for the segmented index when the query contains CJK text; `None` when it has none, or
/// no usable word is left (the regular index is used then).
pub(crate) fn terms(q: &str) -> Option<Vec<String>> {
    const MAX_TERMS: usize = 64;
    if !has_cjk(q) {
        return None;
    }
    let mut terms = for_query(q);
    terms.truncate(MAX_TERMS);
    (!terms.is_empty()).then_some(terms)
}

/// Replaces the index row of the memory at `rowid`.
pub(crate) fn write(conn: &Connection, rowid: i64, title: &str, content: &str) -> Result<()> {
    conn.execute("DELETE FROM memory_fts_seg WHERE rowid=?1", [rowid])
        .map_err(storage)?;
    conn.execute(
        "INSERT INTO memory_fts_seg(rowid, title, content) VALUES (?1,?2,?3)",
        params![rowid, for_index(title), for_index(content)],
    )
    .map_err(storage)?;
    Ok(())
}

/// Indexes every memory again and records the segmentation that built it.
pub(crate) fn rebuild(conn: &Connection) -> Result<()> {
    conn.execute("DELETE FROM memory_fts_seg", [])
        .map_err(storage)?;
    let mut stmt = conn
        .prepare("SELECT rowid, title, content FROM memories")
        .map_err(storage)?;
    let rows: Vec<(i64, String, String)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .map_err(storage)?
        .collect::<rusqlite::Result<_>>()
        .map_err(storage)?;
    for (rowid, title, content) in rows {
        write(conn, rowid, &title, &content)?;
    }
    conn.execute(
        "INSERT INTO fts_meta(key, value) VALUES ('segment_version', ?1)
         ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        [SEGMENT_VERSION],
    )
    .map_err(storage)?;
    Ok(())
}

/// Rebuilds when the index was built by another segmentation (or not at all yet).
pub(crate) fn ensure(conn: &Connection) -> Result<()> {
    let built: Option<String> = conn
        .query_row(
            "SELECT value FROM fts_meta WHERE key='segment_version'",
            [],
            |r| r.get(0),
        )
        .optional()
        .map_err(storage)?;
    if built.as_deref() == Some(SEGMENT_VERSION) {
        return Ok(());
    }
    rebuild(conn)
}

/// The index has one row per memory and was built by the current segmentation.
pub(crate) fn in_step(conn: &Connection) -> bool {
    let count = |t: &str| {
        conn.query_row(&format!("SELECT count(*) FROM {t}"), [], |r| {
            r.get::<_, i64>(0)
        })
    };
    let built: Option<String> = conn
        .query_row(
            "SELECT value FROM fts_meta WHERE key='segment_version'",
            [],
            |r| r.get(0),
        )
        .ok();
    built.as_deref() == Some(SEGMENT_VERSION)
        && matches!((count("memories"), count("memory_fts_seg")), (Ok(a), Ok(b)) if a == b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_chinese_and_keeps_identifiers() {
        let t = std::time::Instant::now();
        let q = for_query("SQLITE_BUSY 怎么避免？tool 名字用什么 prefix");
        eprintln!("{q:?} first use {:?}", t.elapsed());
        assert!(q.contains(&"sqlite_busy".to_string()), "{q:?}");
        assert!(!q.iter().any(|w| w == "怎么" || w == "什么"), "{q:?}");
        let idx = for_index("把 busy_timeout 设为 5 秒，数据库并发读写");
        eprintln!("{idx}");
        assert!(
            idx.contains("busy_timeout") && idx.contains("数据库"),
            "{idx}"
        );
        assert_eq!(for_index("plain english"), "plain english");
    }
}
