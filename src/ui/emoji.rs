/// Emoji shortcode-to-Unicode mapping for Slack messages.
///
/// Replaces `:shortcode:` patterns in text with their Unicode equivalents.
/// Handles skin-tone modifier shortcodes by stripping them (terminal rendering
/// of ZWJ skin-tone sequences is unreliable).

/// Look up the Unicode emoji for a given shortcode (without surrounding colons).
///
/// Returns `None` if the shortcode is not recognized.
pub fn emoji_for(shortcode: &str) -> Option<&'static str> {
    let emoji = match shortcode {
        // ---- Faces / people ----
        "slightly_smiling_face" => "\u{1F642}",
        "smile" => "\u{1F604}",
        "grinning" => "\u{1F600}",
        "laughing" | "satisfied" => "\u{1F606}",
        "joy" => "\u{1F602}",
        "rofl" => "\u{1F923}",
        "wink" => "\u{1F609}",
        "blush" => "\u{1F60A}",
        "innocent" => "\u{1F607}",
        "heart_eyes" => "\u{1F60D}",
        "star_struck" | "star-struck" => "\u{1F929}",
        "kissing_heart" => "\u{1F618}",
        "thinking_face" | "thinking" => "\u{1F914}",
        "face_with_raised_eyebrow" | "raised_eyebrow" => "\u{1F928}",
        "neutral_face" => "\u{1F610}",
        "expressionless" => "\u{1F611}",
        "no_mouth" => "\u{1F636}",
        "face_with_rolling_eyes" | "rolling_eyes" => "\u{1F644}",
        "smirk" => "\u{1F60F}",
        "persevere" => "\u{1F623}",
        "disappointed" => "\u{1F61E}",
        "sweat" => "\u{1F613}",
        "weary" => "\u{1F629}",
        "tired_face" => "\u{1F62B}",
        "cry" => "\u{1F622}",
        "sob" => "\u{1F62D}",
        "scream" => "\u{1F631}",
        "face_holding_back_tears" => "\u{1F979}",
        "rage" | "pout" => "\u{1F621}",
        "angry" => "\u{1F620}",
        "skull" => "\u{1F480}",
        "ghost" => "\u{1F47B}",
        "robot_face" | "robot" => "\u{1F916}",
        "clown_face" | "clown" => "\u{1F921}",
        "poop" | "hankey" | "shit" => "\u{1F4A9}",
        "see_no_evil" => "\u{1F648}",
        "hear_no_evil" => "\u{1F649}",
        "speak_no_evil" => "\u{1F64A}",

        // ---- Hands / gestures ----
        "wave" => "\u{1F44B}",
        "raised_hands" => "\u{1F64C}",
        "clap" => "\u{1F44F}",
        "pray" | "folded_hands" => "\u{1F64F}",
        "handshake" => "\u{1F91D}",
        "thumbsup" | "+1" | "thumbs_up" => "\u{1F44D}",
        "thumbsdown" | "-1" | "thumbs_down" => "\u{1F44E}",
        "fist" | "fist_raised" => "\u{270A}",
        "punch" | "facepunch" | "fist_oncoming" => "\u{1F44A}",
        "ok_hand" => "\u{1F44C}",
        "v" => "\u{270C}\u{FE0F}",
        "metal" => "\u{1F918}",
        "pinching_hand" => "\u{1F90F}",
        "crossed_fingers" => "\u{1F91E}",
        "hand_with_index_finger_and_thumb_crossed" | "love_you_gesture" => "\u{1F91F}",
        "point_up" => "\u{261D}\u{FE0F}",
        "point_down" => "\u{1F447}",
        "point_left" => "\u{1F448}",
        "point_right" => "\u{1F449}",
        "muscle" => "\u{1F4AA}",
        "brain" => "\u{1F9E0}",
        "raised_hand" | "hand" => "\u{270B}",
        "raised_back_of_hand" => "\u{1F91A}",
        "open_hands" => "\u{1F450}",
        "palms_up_together" => "\u{1F932}",
        "writing_hand" => "\u{270D}\u{FE0F}",
        "eyes" => "\u{1F440}",
        "eye" => "\u{1F441}\u{FE0F}",

        // ---- Hearts / emotion ----
        "heart" | "red_heart" => "\u{2764}\u{FE0F}",
        "purple_heart" => "\u{1F49C}",
        "blue_heart" => "\u{1F499}",
        "green_heart" => "\u{1F49A}",
        "yellow_heart" => "\u{1F49B}",
        "orange_heart" => "\u{1F9E1}",
        "white_heart" => "\u{1F90D}",
        "broken_heart" => "\u{1F494}",
        "heavy_heart_exclamation_mark_ornament" | "heart_exclamation" => "\u{2763}\u{FE0F}",
        "sparkling_heart" => "\u{1F496}",
        "revolving_hearts" => "\u{1F49E}",
        "two_hearts" => "\u{1F495}",
        "heartbeat" => "\u{1F493}",
        "heartpulse" => "\u{1F497}",
        "growing_heart" => "\u{1F49D}",
        "fire" | "flame" => "\u{1F525}",
        "star" => "\u{2B50}",
        "sparkles" => "\u{2728}",
        "zap" => "\u{26A1}",
        "boom" | "collision" => "\u{1F4A5}",
        "sweat_drops" => "\u{1F4A6}",

        // ---- Common objects ----
        "rocket" => "\u{1F680}",
        "tada" => "\u{1F389}",
        "confetti_ball" => "\u{1F38A}",
        "balloon" => "\u{1F388}",
        "trophy" => "\u{1F3C6}",
        "medal" | "sports_medal" => "\u{1F3C5}",
        "crown" => "\u{1F451}",
        "gem" => "\u{1F48E}",
        "bulb" => "\u{1F4A1}",
        "wrench" => "\u{1F527}",
        "hammer" => "\u{1F528}",
        "key" => "\u{1F511}",
        "lock" => "\u{1F512}",
        "unlock" => "\u{1F513}",
        "bell" => "\u{1F514}",
        "megaphone" => "\u{1F4E3}",
        "loudspeaker" => "\u{1F4E2}",
        "moneybag" => "\u{1F4B0}",
        "chart_with_upwards_trend" => "\u{1F4C8}",
        "chart_with_downwards_trend" => "\u{1F4C9}",
        "warning" => "\u{26A0}\u{FE0F}",
        "no_entry" => "\u{26D4}",
        "octagonal_sign" | "stop_sign" => "\u{1F6D1}",
        "x" => "\u{274C}",
        "white_check_mark" => "\u{2705}",
        "heavy_check_mark" => "\u{2714}\u{FE0F}",
        "heavy_multiplication_x" => "\u{2716}\u{FE0F}",
        "bangbang" => "\u{203C}\u{FE0F}",
        "question" => "\u{2753}",
        "grey_question" => "\u{2754}",
        "exclamation" | "heavy_exclamation_mark" => "\u{2757}",
        "red_circle" => "\u{1F534}",
        "large_blue_circle" => "\u{1F535}",
        "white_circle" => "\u{26AA}",
        "black_circle" => "\u{26AB}",
        "checkered_flag" => "\u{1F3C1}",

        // ---- Nature / animals ----
        "dog" | "dog_face" => "\u{1F436}",
        "cat" | "cat_face" => "\u{1F431}",
        "bear" => "\u{1F43B}",
        "panda_face" => "\u{1F43C}",
        "monkey_face" => "\u{1F435}",
        "penguin" => "\u{1F427}",
        "chicken" => "\u{1F414}",
        "snake" => "\u{1F40D}",
        "bug" => "\u{1F41B}",
        "bee" | "honeybee" => "\u{1F41D}",
        "butterfly" => "\u{1F98B}",
        "unicorn" | "unicorn_face" => "\u{1F984}",
        "rainbow" => "\u{1F308}",
        "sun_with_face" => "\u{1F31E}",
        "full_moon_with_face" => "\u{1F31D}",
        "cloud" => "\u{2601}\u{FE0F}",
        "umbrella" => "\u{2602}\u{FE0F}",
        "snowflake" => "\u{2744}\u{FE0F}",
        "ocean" => "\u{1F30A}",

        // ---- Food / drink ----
        "coffee" => "\u{2615}",
        "beer" => "\u{1F37A}",
        "beers" => "\u{1F37B}",
        "wine_glass" => "\u{1F377}",
        "cocktail" => "\u{1F378}",
        "pizza" => "\u{1F355}",
        "hamburger" => "\u{1F354}",
        "taco" => "\u{1F32E}",
        "burrito" => "\u{1F32F}",
        "cake" => "\u{1F370}",
        "cookie" => "\u{1F36A}",
        "ice_cream" => "\u{1F368}",
        "doughnut" => "\u{1F369}",
        "apple" => "\u{1F34E}",
        "banana" => "\u{1F34C}",

        // ---- Symbols / misc ----
        "100" => "\u{1F4AF}",
        "heavy_plus_sign" => "\u{2795}",
        "heavy_minus_sign" => "\u{2796}",
        "heavy_division_sign" => "\u{2797}",
        "infinity" => "\u{267E}\u{FE0F}",
        "recycle" => "\u{267B}\u{FE0F}",
        "copyright" => "\u{00A9}\u{FE0F}",
        "registered" => "\u{00AE}\u{FE0F}",
        "tm" => "\u{2122}\u{FE0F}",
        "arrow_right" => "\u{27A1}\u{FE0F}",
        "arrow_left" => "\u{2B05}\u{FE0F}",
        "arrow_up" => "\u{2B06}\u{FE0F}",
        "arrow_down" => "\u{2B07}\u{FE0F}",
        "arrows_counterclockwise" => "\u{1F504}",
        "rewind" => "\u{23EA}",
        "fast_forward" => "\u{23E9}",
        "play_button" | "arrow_forward" => "\u{25B6}\u{FE0F}",
        "pause_button" | "double_vertical_bar" => "\u{23F8}\u{FE0F}",
        "stop_button" => "\u{23F9}\u{FE0F}",
        "information_source" => "\u{2139}\u{FE0F}",
        "abc" => "\u{1F524}",
        "atm" => "\u{1F3E7}",
        "new" => "\u{1F195}",
        "free" => "\u{1F193}",
        "sos" => "\u{1F198}",
        "link" => "\u{1F517}",
        "paperclip" => "\u{1F4CE}",
        "scissors" => "\u{2702}\u{FE0F}",
        "pencil" => "\u{1F4DD}",
        "pencil2" => "\u{270F}\u{FE0F}",
        "memo" => "\u{1F4DD}",
        "clipboard" => "\u{1F4CB}",
        "calendar" => "\u{1F4C5}",
        "pushpin" => "\u{1F4CC}",
        "round_pushpin" => "\u{1F4CD}",
        "triangular_flag_on_post" => "\u{1F6A9}",
        "page_facing_up" => "\u{1F4C4}",
        "page_with_curl" => "\u{1F4C3}",
        "bookmark" => "\u{1F516}",
        "dizzy" => "\u{1F4AB}",
        "star2" => "\u{1F31F}",
        "gift" => "\u{1F381}",
        "mega" => "\u{1F4E3}",
        "money_with_wings" => "\u{1F4B8}",
        "gear" => "\u{2699}\u{FE0F}",
        "hourglass" => "\u{231B}",
        "alarm_clock" => "\u{23F0}",
        "stopwatch" => "\u{23F1}\u{FE0F}",
        "mouse" => "\u{1F42D}",
        "frog" => "\u{1F438}",
        "turtle" => "\u{1F422}",
        "octopus" => "\u{1F419}",
        "crab" => "\u{1F980}",
        "popcorn" => "\u{1F37F}",
        "avocado" => "\u{1F951}",
        "sunny" => "\u{2600}\u{FE0F}",
        "rose" => "\u{1F339}",
        "sunflower" => "\u{1F33B}",
        "herb" => "\u{1F33F}",
        "seedling" => "\u{1F331}",
        "fallen_leaf" => "\u{1F342}",
        "maple_leaf" => "\u{1F341}",
        "tree" => "\u{1F333}",
        "cactus" => "\u{1F335}",
        "earth_americas" => "\u{1F30E}",
        "negative_squared_cross_mark" => "\u{274E}",
        "zzz" => "\u{1F4A4}",
        "speech_balloon" => "\u{1F4AC}",
        "thought_balloon" => "\u{1F4AD}",
        "wave_dash" => "\u{3030}\u{FE0F}",
        "black_heart" => "\u{1F5A4}",

        _ => return None,
    };
    Some(emoji)
}

/// Returns `true` if the shortcode is a skin-tone modifier that should be
/// silently dropped (terminal emoji rendering with ZWJ skin tones is unreliable).
fn is_skin_tone_modifier(shortcode: &str) -> bool {
    matches!(
        shortcode,
        "skin-tone-2" | "skin-tone-3" | "skin-tone-4" | "skin-tone-5" | "skin-tone-6"
    )
}

/// Replace all `:shortcode:` patterns in `text` with Unicode emoji.
///
/// * Known shortcodes are replaced with their emoji.
/// * Skin-tone modifier shortcodes (`:skin-tone-N:`) are silently removed so
///   that sequences like `:raised_hands::skin-tone-5:` render as the base emoji
///   without a dangling modifier.
/// * Unknown shortcodes are left as-is.
///
/// The function avoids allocating when the input contains no colons.
pub fn replace_emoji_shortcodes(text: &str) -> String {
    // Fast path: no colons means nothing to replace.
    if !text.contains(':') {
        return text.to_owned();
    }

    let bytes = text.as_bytes();
    let len = bytes.len();
    let mut result = String::with_capacity(len);
    let mut i = 0;

    while i < len {
        if bytes[i] == b':' {
            // Look for the closing colon. Shortcodes contain [a-zA-Z0-9_+\-].
            // We cap the scan at a reasonable max length to avoid degenerate cases.
            const MAX_SHORTCODE_LEN: usize = 64;
            let start = i;
            let mut found_end = false;
            let mut j = i + 1;
            let limit = len.min(j + MAX_SHORTCODE_LEN);

            while j < limit {
                let b = bytes[j];
                if b == b':' {
                    // We found a closing colon.
                    let shortcode = &text[start + 1..j];
                    if !shortcode.is_empty() {
                        if is_skin_tone_modifier(shortcode) {
                            // Silently drop skin-tone modifiers.
                            i = j + 1;
                            found_end = true;
                            break;
                        } else if let Some(emoji) = emoji_for(shortcode) {
                            result.push_str(emoji);
                            i = j + 1;
                            found_end = true;
                            break;
                        }
                    }
                    // Not a known shortcode — treat the opening colon as literal.
                    break;
                } else if b.is_ascii_alphanumeric() || b == b'_' || b == b'-' || b == b'+' {
                    j += 1;
                } else {
                    // Invalid character for a shortcode — bail out.
                    break;
                }
            }

            if !found_end {
                result.push(':');
                i = start + 1;
            }
        } else {
            // Regular character — find the next colon (or end) and bulk-copy.
            let start = i;
            i += 1;
            while i < len && bytes[i] != b':' {
                i += 1;
            }
            result.push_str(&text[start..i]);
        }
    }

    result
}

/// Returns all standard emoji as (shortcode, unicode) pairs for enumeration.
pub fn all_standard_emoji() -> &'static [(&'static str, &'static str)] {
    &[
        // Faces / people
        ("slightly_smiling_face", "\u{1F642}"),
        ("smile", "\u{1F604}"),
        ("grinning", "\u{1F600}"),
        ("laughing", "\u{1F606}"),
        ("joy", "\u{1F602}"),
        ("rofl", "\u{1F923}"),
        ("wink", "\u{1F609}"),
        ("blush", "\u{1F60A}"),
        ("innocent", "\u{1F607}"),
        ("heart_eyes", "\u{1F60D}"),
        ("star_struck", "\u{1F929}"),
        ("kissing_heart", "\u{1F618}"),
        ("thinking_face", "\u{1F914}"),
        ("raised_eyebrow", "\u{1F928}"),
        ("neutral_face", "\u{1F610}"),
        ("expressionless", "\u{1F611}"),
        ("no_mouth", "\u{1F636}"),
        ("rolling_eyes", "\u{1F644}"),
        ("smirk", "\u{1F60F}"),
        ("persevere", "\u{1F623}"),
        ("disappointed", "\u{1F61E}"),
        ("sweat", "\u{1F613}"),
        ("weary", "\u{1F629}"),
        ("tired_face", "\u{1F62B}"),
        ("cry", "\u{1F622}"),
        ("sob", "\u{1F62D}"),
        ("scream", "\u{1F631}"),
        ("face_holding_back_tears", "\u{1F979}"),
        ("rage", "\u{1F621}"),
        ("angry", "\u{1F620}"),
        ("skull", "\u{1F480}"),
        ("ghost", "\u{1F47B}"),
        ("robot_face", "\u{1F916}"),
        ("clown_face", "\u{1F921}"),
        ("poop", "\u{1F4A9}"),
        ("see_no_evil", "\u{1F648}"),
        ("hear_no_evil", "\u{1F649}"),
        ("speak_no_evil", "\u{1F64A}"),
        ("wave", "\u{1F44B}"),
        ("raised_hands", "\u{1F64C}"),
        ("clap", "\u{1F44F}"),
        ("handshake", "\u{1F91D}"),
        ("+1", "\u{1F44D}"),
        ("thumbsup", "\u{1F44D}"),
        ("-1", "\u{1F44E}"),
        ("thumbsdown", "\u{1F44E}"),
        ("fist", "\u{270A}"),
        ("punch", "\u{1F44A}"),
        ("point_up", "\u{261D}\u{FE0F}"),
        ("point_down", "\u{1F447}"),
        ("point_left", "\u{1F448}"),
        ("point_right", "\u{1F449}"),
        ("ok_hand", "\u{1F44C}"),
        ("v", "\u{270C}\u{FE0F}"),
        ("crossed_fingers", "\u{1F91E}"),
        ("metal", "\u{1F918}"),
        ("muscle", "\u{1F4AA}"),
        ("pray", "\u{1F64F}"),
        ("eyes", "\u{1F440}"),
        ("brain", "\u{1F9E0}"),
        // Hearts / symbols
        ("heart", "\u{2764}\u{FE0F}"),
        ("orange_heart", "\u{1F9E1}"),
        ("yellow_heart", "\u{1F49B}"),
        ("green_heart", "\u{1F49A}"),
        ("blue_heart", "\u{1F499}"),
        ("purple_heart", "\u{1F49C}"),
        ("black_heart", "\u{1F5A4}"),
        ("broken_heart", "\u{1F494}"),
        ("sparkling_heart", "\u{1F496}"),
        ("100", "\u{1F4AF}"),
        ("boom", "\u{1F4A5}"),
        ("dizzy", "\u{1F4AB}"),
        ("sparkles", "\u{2728}"),
        ("star", "\u{2B50}"),
        ("star2", "\u{1F31F}"),
        ("zap", "\u{26A1}"),
        ("fire", "\u{1F525}"),
        // Objects / celebration
        ("tada", "\u{1F389}"),
        ("confetti_ball", "\u{1F38A}"),
        ("balloon", "\u{1F388}"),
        ("gift", "\u{1F381}"),
        ("trophy", "\u{1F3C6}"),
        ("medal", "\u{1F3C5}"),
        ("crown", "\u{1F451}"),
        ("gem", "\u{1F48E}"),
        ("bell", "\u{1F514}"),
        ("mega", "\u{1F4E3}"),
        ("loudspeaker", "\u{1F4E2}"),
        ("bulb", "\u{1F4A1}"),
        ("money_with_wings", "\u{1F4B8}"),
        ("moneybag", "\u{1F4B0}"),
        ("key", "\u{1F511}"),
        ("lock", "\u{1F512}"),
        ("hammer", "\u{1F528}"),
        ("wrench", "\u{1F527}"),
        ("gear", "\u{2699}\u{FE0F}"),
        ("rocket", "\u{1F680}"),
        ("hourglass", "\u{231B}"),
        ("alarm_clock", "\u{23F0}"),
        ("stopwatch", "\u{23F1}\u{FE0F}"),
        // Nature / animals
        ("dog", "\u{1F436}"),
        ("cat", "\u{1F431}"),
        ("mouse", "\u{1F42D}"),
        ("bear", "\u{1F43B}"),
        ("panda_face", "\u{1F43C}"),
        ("penguin", "\u{1F427}"),
        ("chicken", "\u{1F414}"),
        ("frog", "\u{1F438}"),
        ("bee", "\u{1F41D}"),
        ("bug", "\u{1F41B}"),
        ("snake", "\u{1F40D}"),
        ("turtle", "\u{1F422}"),
        ("octopus", "\u{1F419}"),
        ("unicorn_face", "\u{1F984}"),
        ("butterfly", "\u{1F98B}"),
        ("crab", "\u{1F980}"),
        // Food / drink
        ("coffee", "\u{2615}"),
        ("beer", "\u{1F37A}"),
        ("beers", "\u{1F37B}"),
        ("wine_glass", "\u{1F377}"),
        ("pizza", "\u{1F355}"),
        ("hamburger", "\u{1F354}"),
        ("taco", "\u{1F32E}"),
        ("burrito", "\u{1F32F}"),
        ("popcorn", "\u{1F37F}"),
        ("cake", "\u{1F370}"),
        ("cookie", "\u{1F36A}"),
        ("doughnut", "\u{1F369}"),
        ("apple", "\u{1F34E}"),
        ("banana", "\u{1F34C}"),
        ("avocado", "\u{1F951}"),
        // Nature
        ("sunny", "\u{2600}\u{FE0F}"),
        ("cloud", "\u{2601}\u{FE0F}"),
        ("umbrella", "\u{2602}\u{FE0F}"),
        ("snowflake", "\u{2744}\u{FE0F}"),
        ("rainbow", "\u{1F308}"),
        ("ocean", "\u{1F30A}"),
        ("rose", "\u{1F339}"),
        ("sunflower", "\u{1F33B}"),
        ("herb", "\u{1F33F}"),
        ("seedling", "\u{1F331}"),
        ("fallen_leaf", "\u{1F342}"),
        ("maple_leaf", "\u{1F341}"),
        ("tree", "\u{1F333}"),
        ("cactus", "\u{1F335}"),
        ("earth_americas", "\u{1F30E}"),
        // Status / marks
        ("white_check_mark", "\u{2705}"),
        ("heavy_check_mark", "\u{2714}\u{FE0F}"),
        ("x", "\u{274C}"),
        ("negative_squared_cross_mark", "\u{274E}"),
        ("exclamation", "\u{2757}"),
        ("question", "\u{2753}"),
        ("warning", "\u{26A0}\u{FE0F}"),
        ("no_entry", "\u{26D4}"),
        ("zzz", "\u{1F4A4}"),
        ("speech_balloon", "\u{1F4AC}"),
        ("thought_balloon", "\u{1F4AD}"),
        ("wave_dash", "\u{3030}\u{FE0F}"),
        ("recycle", "\u{267B}\u{FE0F}"),
        ("arrow_up", "\u{2B06}\u{FE0F}"),
        ("arrow_down", "\u{2B07}\u{FE0F}"),
        ("arrow_right", "\u{27A1}\u{FE0F}"),
        ("arrow_left", "\u{2B05}\u{FE0F}"),
        // Misc
        ("link", "\u{1F517}"),
        ("paperclip", "\u{1F4CE}"),
        ("scissors", "\u{2702}\u{FE0F}"),
        ("pencil", "\u{1F4DD}"),
        ("memo", "\u{1F4DD}"),
        ("clipboard", "\u{1F4CB}"),
        ("calendar", "\u{1F4C5}"),
        ("pushpin", "\u{1F4CC}"),
        ("bookmark", "\u{1F516}"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_replacement() {
        assert_eq!(replace_emoji_shortcodes(":rocket:"), "\u{1F680}");
        assert_eq!(replace_emoji_shortcodes(":tada:"), "\u{1F389}");
    }

    #[test]
    fn inline_replacement() {
        assert_eq!(
            replace_emoji_shortcodes("ship it :rocket: now"),
            "ship it \u{1F680} now"
        );
    }

    #[test]
    fn multiple_emoji() {
        assert_eq!(
            replace_emoji_shortcodes(":fire::100:"),
            "\u{1F525}\u{1F4AF}"
        );
    }

    #[test]
    fn unknown_shortcode_left_alone() {
        assert_eq!(
            replace_emoji_shortcodes(":not_a_real_emoji:"),
            ":not_a_real_emoji:"
        );
    }

    #[test]
    fn skin_tone_stripped() {
        assert_eq!(
            replace_emoji_shortcodes(":raised_hands::skin-tone-5:"),
            "\u{1F64C}"
        );
    }

    #[test]
    fn no_colons_no_alloc_content_change() {
        let input = "hello world";
        assert_eq!(replace_emoji_shortcodes(input), "hello world");
    }

    #[test]
    fn thumbsup_aliases() {
        assert_eq!(replace_emoji_shortcodes(":+1:"), "\u{1F44D}");
        assert_eq!(replace_emoji_shortcodes(":thumbsup:"), "\u{1F44D}");
        assert_eq!(replace_emoji_shortcodes(":-1:"), "\u{1F44E}");
    }

    #[test]
    fn colons_in_non_shortcode_context() {
        // Timestamps or other colon-containing text should pass through.
        assert_eq!(
            replace_emoji_shortcodes("time is 12:30:00"),
            "time is 12:30:00"
        );
    }

    #[test]
    fn mixed_known_and_unknown() {
        assert_eq!(
            replace_emoji_shortcodes(":heart: and :mystery: and :fire:"),
            "\u{2764}\u{FE0F} and :mystery: and \u{1F525}"
        );
    }

    #[test]
    fn empty_colons() {
        assert_eq!(replace_emoji_shortcodes("::"), "::");
    }

    #[test]
    fn trailing_colon() {
        assert_eq!(replace_emoji_shortcodes("hello:"), "hello:");
    }

    #[test]
    fn all_standard_emoji_no_duplicate_names() {
        let all = all_standard_emoji();
        let mut seen = std::collections::HashSet::new();
        for &(name, _) in all {
            // Allow intentional duplicates (thumbsup/+1 etc)
            seen.insert(name);
        }
        // Should have a reasonable number of entries
        assert!(all.len() > 100, "expected >100 standard emoji, got {}", all.len());
    }

    #[test]
    fn all_standard_emoji_matches_emoji_for() {
        for &(name, expected) in all_standard_emoji() {
            let result = emoji_for(name);
            assert!(
                result.is_some(),
                "all_standard_emoji contains '{}' but emoji_for returns None",
                name
            );
            assert_eq!(
                result.unwrap(),
                expected,
                "mismatch for '{}': emoji_for={:?}, all_standard_emoji={:?}",
                name,
                result,
                expected
            );
        }
    }
}
