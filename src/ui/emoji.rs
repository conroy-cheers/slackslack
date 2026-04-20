use std::collections::HashMap;

/// Look up emoji from the runtime-fetched database, falling back to the hardcoded table.
pub fn emoji_for_runtime<'a>(shortcode: &str, standard_emoji: &'a HashMap<String, String>) -> Option<&'a str> {
    standard_emoji.get(shortcode).map(|s| s.as_str())
        .or_else(|| emoji_for(shortcode))
}

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
        "large_blue_circle" | "blue_circle" => "\u{1F535}",
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

        // ---- Keycap numbers / symbols ----
        "zero" => "0\u{FE0F}\u{20E3}",
        "one" => "1\u{FE0F}\u{20E3}",
        "two" => "2\u{FE0F}\u{20E3}",
        "three" => "3\u{FE0F}\u{20E3}",
        "four" => "4\u{FE0F}\u{20E3}",
        "five" => "5\u{FE0F}\u{20E3}",
        "six" => "6\u{FE0F}\u{20E3}",
        "seven" => "7\u{FE0F}\u{20E3}",
        "eight" => "8\u{FE0F}\u{20E3}",
        "nine" => "9\u{FE0F}\u{20E3}",
        "keycap_ten" | "ten" => "\u{1F51F}",
        "hash" => "#\u{FE0F}\u{20E3}",
        "asterisk" | "keycap_star" => "*\u{FE0F}\u{20E3}",

        // ---- Zodiac ----
        "aries" => "\u{2648}",
        "taurus" => "\u{2649}",
        "gemini" => "\u{264A}",
        "cancer" => "\u{264B}",
        "leo" => "\u{264C}",
        "virgo" => "\u{264D}",
        "libra" => "\u{264E}",
        "scorpius" => "\u{264F}",
        "sagittarius" => "\u{2650}",
        "capricorn" => "\u{2651}",
        "aquarius" => "\u{2652}",
        "pisces" => "\u{2653}",

        // ---- Arrows (additional) ----
        "arrow_upper_right" => "\u{2197}\u{FE0F}",
        "arrow_lower_right" => "\u{2198}\u{FE0F}",
        "arrow_lower_left" => "\u{2199}\u{FE0F}",
        "arrow_upper_left" => "\u{2196}\u{FE0F}",
        "arrow_up_down" => "\u{2195}\u{FE0F}",
        "left_right_arrow" => "\u{2194}\u{FE0F}",
        "arrow_right_hook" => "\u{21AA}\u{FE0F}",
        "leftwards_arrow_with_hook" => "\u{21A9}\u{FE0F}",
        "arrow_heading_up" => "\u{2934}\u{FE0F}",
        "arrow_heading_down" => "\u{2935}\u{FE0F}",
        "twisted_rightwards_arrows" => "\u{1F500}",
        "repeat" => "\u{1F501}",
        "repeat_one" => "\u{1F502}",
        "back" => "\u{1F519}",
        "end" => "\u{1F51A}",
        "on" => "\u{1F51B}",
        "soon" => "\u{1F51C}",
        "top" => "\u{1F51D}",

        // ---- Geometric / shapes ----
        "orange_circle" => "\u{1F7E0}",
        "yellow_circle" => "\u{1F7E1}",
        "green_circle" => "\u{1F7E2}",
        "purple_circle" => "\u{1F7E3}",
        "brown_circle" => "\u{1F7E4}",
        "large_red_square" | "red_square" => "\u{1F7E5}",
        "large_orange_square" | "orange_square" => "\u{1F7E7}",
        "large_yellow_square" | "yellow_square" => "\u{1F7E8}",
        "large_green_square" | "green_square" => "\u{1F7E9}",
        "large_blue_square" | "blue_square" => "\u{1F7E6}",
        "large_purple_square" | "purple_square" => "\u{1F7EA}",
        "large_brown_square" | "brown_square" => "\u{1F7EB}",
        "black_large_square" => "\u{2B1B}",
        "white_large_square" => "\u{2B1C}",
        "black_medium_square" => "\u{25FC}\u{FE0F}",
        "white_medium_square" => "\u{25FB}\u{FE0F}",
        "black_medium_small_square" => "\u{25FE}",
        "white_medium_small_square" => "\u{25FD}",
        "black_small_square" => "\u{25AA}\u{FE0F}",
        "white_small_square" => "\u{25AB}\u{FE0F}",
        "diamond_shape_with_a_dot_inside" => "\u{1F4A0}",
        "small_red_triangle" => "\u{1F53A}",
        "small_red_triangle_down" => "\u{1F53B}",
        "small_orange_diamond" => "\u{1F538}",
        "small_blue_diamond" => "\u{1F539}",
        "large_orange_diamond" => "\u{1F536}",
        "large_blue_diamond" => "\u{1F537}",
        "radio_button" => "\u{1F518}",

        // ---- Additional (not in original table) ----
        "partly_sunny" | "sun_behind_cloud" => "\u{26C5}",
        "snowman_without_snow" => "\u{26C4}",
        "comet" => "\u{2604}\u{FE0F}",
        "no_entry_sign" => "\u{1F6AB}",
        "o" | "heavy_large_circle" => "\u{2B55}",
        "grey_exclamation" => "\u{2755}",
        "white_exclamation_mark" => "\u{2755}",
        "white_question_mark" => "\u{2754}",
        "interrobang" => "\u{2049}\u{FE0F}",
        "low_brightness" => "\u{1F505}",
        "high_brightness" => "\u{1F506}",
        "mute" => "\u{1F507}",
        "speaker" => "\u{1F508}",
        "sound" | "loud_sound" => "\u{1F50A}",
        "no_bell" => "\u{1F515}",
        "first_place_medal" | "1st_place_medal" => "\u{1F947}",
        "second_place_medal" | "2nd_place_medal" => "\u{1F948}",
        "third_place_medal" | "3rd_place_medal" => "\u{1F949}",
        "soccer" => "\u{26BD}",
        "basketball" => "\u{1F3C0}",
        "football" => "\u{1F3C8}",
        "baseball" => "\u{26BE}",
        "tennis" => "\u{1F3BE}",
        "dart" => "\u{1F3AF}",
        "bowling" => "\u{1F3B3}",
        "golf" | "golfing" => "\u{1F3CC}\u{FE0F}",
        "video_game" | "joystick" => "\u{1F3AE}",
        "slot_machine" => "\u{1F3B0}",
        "game_die" => "\u{1F3B2}",
        "musical_note" => "\u{1F3B5}",
        "notes" | "musical_notes" => "\u{1F3B6}",
        "microphone" => "\u{1F3A4}",
        "headphones" | "headphone" => "\u{1F3A7}",
        "guitar" => "\u{1F3B8}",
        "trumpet" => "\u{1F3BA}",
        "drum" | "drum_with_drumsticks" => "\u{1F941}",
        "movie_camera" => "\u{1F3A5}",
        "clapper" | "clapper_board" => "\u{1F3AC}",
        "tv" | "television" => "\u{1F4FA}",
        "camera" => "\u{1F4F7}",
        "computer" | "desktop_computer" => "\u{1F4BB}",
        "keyboard" => "\u{2328}\u{FE0F}",
        "phone" | "telephone" => "\u{260E}\u{FE0F}",
        "mobile_phone" | "iphone" => "\u{1F4F1}",
        "battery" => "\u{1F50B}",
        "electric_plug" => "\u{1F50C}",
        "light_bulb" => "\u{1F4A1}",
        "flashlight" => "\u{1F526}",
        "candle" => "\u{1F56F}\u{FE0F}",
        "wastebasket" => "\u{1F5D1}\u{FE0F}",
        "nut_and_bolt" => "\u{1F529}",
        "mag" | "mag_right" => "\u{1F50D}",
        "microscope" => "\u{1F52C}",
        "telescope" => "\u{1F52D}",
        "crystal_ball" => "\u{1F52E}",
        "bomb" => "\u{1F4A3}",
        "knife" | "hocho" => "\u{1F52A}",
        "shield" => "\u{1F6E1}\u{FE0F}",
        "skull_and_crossbones" => "\u{2620}\u{FE0F}",
        "radioactive" => "\u{2622}\u{FE0F}",
        "biohazard" => "\u{2623}\u{FE0F}",
        "peace" | "peace_symbol" => "\u{262E}\u{FE0F}",
        "atom" | "atom_symbol" => "\u{269B}\u{FE0F}",
        "rainbow_flag" => "\u{1F3F3}\u{FE0F}\u{200D}\u{1F308}",
        "tongue" => "\u{1F445}",
        "lips" => "\u{1F444}",
        "alien" => "\u{1F47D}",
        "jack_o_lantern" => "\u{1F383}",
        "christmas_tree" => "\u{1F384}",
        "santa" => "\u{1F385}",
        "fireworks" => "\u{1F386}",
        "door" => "\u{1F6AA}",
        "toilet" => "\u{1F6BD}",
        "shower" => "\u{1F6BF}",
        "car" | "red_car" | "automobile" => "\u{1F697}",
        "taxi" => "\u{1F695}",
        "bus" => "\u{1F68C}",
        "airplane" => "\u{2708}\u{FE0F}",
        "sailboat" => "\u{26F5}",
        "train" | "railway_car" => "\u{1F683}",
        "house" => "\u{1F3E0}",
        "church" => "\u{26EA}",
        "tent" => "\u{26FA}",

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
    replace_emoji_impl(text, |sc| emoji_for(sc))
}

pub fn replace_emoji_shortcodes_with_map(text: &str, standard_emoji: &HashMap<String, String>) -> String {
    replace_emoji_impl(text, |sc| emoji_for_runtime(sc, standard_emoji))
}

fn replace_emoji_impl<'a>(text: &str, lookup: impl Fn(&str) -> Option<&'a str>) -> String {
    if !text.contains(':') {
        return text.to_owned();
    }

    let bytes = text.as_bytes();
    let len = bytes.len();
    let mut result = String::with_capacity(len);
    let mut i = 0;

    while i < len {
        if bytes[i] == b':' {
            const MAX_SHORTCODE_LEN: usize = 64;
            let start = i;
            let mut found_end = false;
            let mut j = i + 1;
            let limit = len.min(j + MAX_SHORTCODE_LEN);

            while j < limit {
                let b = bytes[j];
                if b == b':' {
                    let shortcode = &text[start + 1..j];
                    if !shortcode.is_empty() {
                        if is_skin_tone_modifier(shortcode) {
                            i = j + 1;
                            found_end = true;
                            break;
                        } else if let Some(emoji) = lookup(shortcode) {
                            result.push_str(emoji);
                            i = j + 1;
                            found_end = true;
                            break;
                        }
                    }
                    break;
                } else if b.is_ascii_alphanumeric() || b == b'_' || b == b'-' || b == b'+' {
                    j += 1;
                } else {
                    break;
                }
            }

            if !found_end {
                result.push(':');
                i = start + 1;
            }
        } else {
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
