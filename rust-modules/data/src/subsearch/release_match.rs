//! A local release-name estimate, never a timing/confidence measurement. Fixed evidence weights
//! leave missing fields unearned: title 35, episode/year 25, source 20, group 15, resolution 5.
//! Explicit episode/year/edition/source conflicts override otherwise convincing shared words.

use std::collections::BTreeSet;

#[derive(Default)]
struct Release {
    title: BTreeSet<String>,
    episode: Option<String>,
    year: Option<u16>,
    source: Option<&'static str>,
    group: Option<String>,
    resolution: Option<String>,
    edition: BTreeSet<&'static str>,
}

/// `None` means there is no usable filename/release title. Scores are deterministic, cheap, and
/// deliberately independent of downloads. Even 100 means matching names, not verified sync.
pub fn score(filename: &str, subtitle: &str) -> Option<u8> {
    let video = parse(filename);
    let sub = parse(subtitle);
    if video.title.is_empty() || sub.title.is_empty() { return None; }
    if matches!((&video.episode, &sub.episode), (Some(a), Some(b)) if a != b) {
        return Some(0);
    }
    if video.episode.is_none() && sub.episode.is_none()
        && matches!((video.year, sub.year), (Some(a), Some(b)) if a != b) {
        return Some(0);
    }
    let overlap = video.title.intersection(&sub.title).count();
    if overlap == 0 { return Some(0); }
    // Dice overlap: punctuation/case/repeated words do not inflate the title contribution.
    let mut points = (70 * overlap / (video.title.len() + sub.title.len())) as u8;
    if video.episode.is_some() {
        if video.episode == sub.episode { points += 25; }
    } else if video.year.is_some() && video.year == sub.year && sub.episode.is_none() {
        points += 25;
    }
    if video.source.is_some() && video.source == sub.source { points += 20; }
    if video.group.is_some() && video.group == sub.group { points += 15; }
    if video.resolution.is_some() && video.resolution == sub.resolution { points += 5; }
    // Shared technical tags cannot disguise a substantially different title.
    if overlap * 2 < video.title.len().max(sub.title.len()) { points = points.min(25); }
    if video.edition != sub.edition { points = points.min(20); }
    if video.source.is_some() && sub.source.is_some() && video.source != sub.source {
        points = points.min(45);
    }
    if video.episode.is_some() != sub.episode.is_some() { points = points.min(35); }
    Some(points)
}

fn stem(value: &str) -> String {
    let leaf = value.rsplit(['/', '\\']).next().unwrap_or("").trim().to_lowercase();
    let (base, ext) = leaf.rsplit_once('.').unwrap_or((&leaf, ""));
    if matches!(ext, "mkv" | "mp4" | "avi" | "m4v" | "mov" | "ts" | "m2ts"
        | "srt" | "ass" | "ssa" | "vtt" | "sub") { base.into() } else { leaf }
}

fn tokens(value: &str) -> Vec<String> {
    value.split(|c: char| !c.is_alphanumeric()).filter(|s| !s.is_empty()).map(str::to_string).collect()
}

/// Both scene episode spellings, including `S01E01E02`. A multi-episode release must match the
/// entire episode identity; sharing just the first episode is not sufficient.
fn episode(token: &str) -> Option<String> {
    if !token.is_ascii() { return None; }
    if let Some(s) = token.strip_prefix('s') {
        let mut parts = s.split('e');
        let season = parts.next()?.parse::<u16>().ok()?;
        let episodes: Vec<u16> = parts.map(str::parse).collect::<Result<_, _>>().ok()?;
        if episodes.is_empty() { return None; }
        return Some(format!("{season}:{episodes:?}"));
    }
    let (season, ep) = token.split_once('x')?;
    Some(format!("{}:{:?}", season.parse::<u16>().ok()?, [ep.parse::<u16>().ok()?]))
}

fn resolution(token: &str) -> bool {
    matches!(token, "480p" | "576p" | "720p" | "1080p" | "1080i" | "2160p" | "4320p" | "4k" | "8k")
}

fn source(words: &[String], i: usize) -> Option<&'static str> {
    let word = words[i].as_str();
    let next = words.get(i + 1).map(String::as_str);
    match (word, next) {
        ("webdl", _) | ("web", Some("dl")) => Some("webdl"),
        ("webrip", _) | ("web", Some("rip")) => Some("webrip"),
        ("bluray" | "bdrip" | "brrip", _) | ("blu", Some("ray")) => Some("bluray"),
        ("hdtv", _) => Some("hdtv"),
        ("dvd" | "dvdrip", _) => Some("dvd"),
        _ => None,
    }
}

fn technical(word: &str) -> bool {
    matches!(word, "x264" | "x265" | "h264" | "h265" | "hevc" | "avc" | "aac" | "ac3"
        | "eac3" | "ddp" | "dts" | "truehd" | "remux" | "hdr" | "hdr10" | "dv" | "proper"
        | "repack" | "10bit" | "8bit")
}

fn parse(value: &str) -> Release {
    let stem = stem(value);
    let words = tokens(&stem);
    let mut release = Release::default();
    // Use the last year before technical/episode tags, preserving numeric titles such as
    // Blade Runner 2049 (2017) instead of mistaking the title's number for its release year.
    let year_index = words.iter().enumerate()
        .take_while(|(i, w)| episode(w).is_none() && source(&words, *i).is_none()
            && !resolution(w) && !technical(w))
        .filter(|(i, w)| *i > 0 && w.parse::<u16>().is_ok_and(|y| (1888..=2100).contains(&y)))
        .map(|(i, _)| i).last();
    let mut boundary = words.len();
    for (i, word) in words.iter().enumerate() {
        let ep = episode(word).or_else(|| words.get(i + 1).and_then(|next| episode(&format!("{word}{next}"))));
        let year = word.parse::<u16>().ok().filter(|_| year_index == Some(i));
        let src = source(&words, i);
        let res = resolution(word);
        let edition = match word.as_str() {
            "extended" => Some("extended"),
            "unrated" => Some("unrated"),
            "uncut" => Some("uncut"),
            "theatrical" => Some("theatrical"),
            "directors" | "director" if words.get(i + 1).is_some_and(|w| w == "cut" || w == "s") => Some("directors"),
            _ => None,
        };
        if ep.is_some() || year.is_some() || src.is_some() || res || technical(word) || edition.is_some() {
            boundary = boundary.min(i);
        }
        if ep.is_some() && release.episode.is_none() { release.episode = ep; }
        if year.is_some() && release.year.is_none() { release.year = year; }
        if src.is_some() && release.source.is_none() { release.source = src; }
        if res { release.resolution = Some(match word.as_str() {
            "4k" => "2160p".into(), "8k" => "4320p".into(), _ => word.clone(),
        }); }
        if let Some(edition) = edition { release.edition.insert(edition); }
    }
    release.title.extend(words[..boundary].iter().cloned());
    // A suffix is a group only after a recognized release marker. This avoids treating a
    // hyphenated film title (Spider-Man) or WEB-DL itself as a release group.
    if let Some((prefix, suffix)) = stem.rsplit_once('-') {
        let group = tokens(suffix);
        if boundary < tokens(prefix).len() && group.len() == 1 && group[0].len() >= 2
            && group[0] != "dl" && group[0] != "rip" && !technical(&group[0]) && !resolution(&group[0]) {
            release.group = Some(group[0].clone());
        }
    }
    if release.title.len() == 1 && matches!(release.title.first().map(String::as_str),
        Some("english" | "eng" | "en" | "subtitles" | "unknown")) { release.title.clear(); }
    release
}

#[cfg(test)]
mod tests {
    use super::score;

    const VIDEO: &str = "/media/The.Movie.2026.1080p.WEB-DL.x264-GROUP.mkv";

    #[test]
    fn exact_release_earns_all_evidence_despite_case_punctuation_and_extension() {
        assert_eq!(score(VIDEO, "the movie 2026 1080p web-dl x264-group.srt"), Some(100));
        assert_eq!(score("D:\\media\\電影.2026.4K.BluRay-GROUP.mkv",
            "電影.2026.2160p.BluRay-GROUP"), Some(100));
    }

    #[test]
    fn missing_evidence_is_not_an_exact_match_and_unusable_names_are_unknown() {
        assert_eq!(score(VIDEO, "The Movie"), Some(35));
        for (file, sub) in [("", "release"), (VIDEO, ""), (VIDEO, "English"), (".mkv", "release")] {
            assert_eq!(score(file, sub), None);
        }
    }

    #[test]
    fn popular_wrong_release_cannot_win_on_shared_technical_words() {
        assert_eq!(score(VIDEO, "Other.Film.2026.1080p.WEB-DL.x264-GROUP"), Some(0));
        assert_eq!(score(VIDEO, "The.Movie.2025.1080p.WEB-DL.x264-GROUP"), Some(0));
        assert!(score(VIDEO, "The.Movie.2026.1080p.BluRay.x264-GROUP").unwrap() <= 45);
        assert!(score(VIDEO, "The.Movie.2026.Extended.1080p.WEB-DL.x264-GROUP").unwrap() <= 20);
    }

    #[test]
    fn episode_identity_is_strict_and_accepts_alternate_scene_spelling() {
        let video = "Show.Name.S01E02.1080p.WEB-DL-GROUP.mkv";
        assert_eq!(score(video, "Show.Name.1x02.1080p.WEB-DL-GROUP"), Some(100));
        for ep in ["S01E03", "S02E02", "S01E02E03"] {
            assert_eq!(score(video, &format!("Show.Name.{ep}.1080p.WEB-DL-GROUP")), Some(0));
        }
        assert!(score(video, "Show.Name.1080p.WEB-DL-GROUP").unwrap() <= 35);
    }

    #[test]
    fn numeric_titles_are_preserved_and_episode_years_do_not_override_episode_identity() {
        assert_eq!(score("Blade.Runner.2049.2017.1080p.WEB-DL-GROUP.mkv",
            "Blade Runner 2049 2017 1080p WEB-DL-GROUP"), Some(100));
        assert_eq!(score("1917.2019.1080p.WEB-DL-GROUP.mkv",
            "1917.2019.1080p.WEB-DL-GROUP"), Some(100));
        assert_eq!(score("Show.2024.S01E02.1080p.WEB-DL-GROUP.mkv",
            "Show.2025.S01E02.1080p.WEB-DL-GROUP"), Some(100));
    }

    #[test]
    fn source_group_and_resolution_earn_independent_weights() {
        assert_eq!(score(VIDEO, "The.Movie.2026.1080p.WEB-DL.x265-OTHER"), Some(85));
        assert_eq!(score(VIDEO, "The.Movie.2026.720p.WEB-DL.x265-GROUP"), Some(95));
        assert_eq!(score(VIDEO, "The.Movie.2026"), Some(60));
        assert_eq!(score("Spider-Man.mkv", "Spider Man.srt"), Some(35));
    }
}
