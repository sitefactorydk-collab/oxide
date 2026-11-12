//! Six-word pairing codes.
//!
//! Each word is one of 256 (8 bits), so a code holds 48 random bits. A code lives 5 minutes
//! and allows 5 wrong tries, so guessing it is hopeless; it is only ever exchanged once for a
//! long random token.

use rand::Rng;

/// The words codes are made of: short, common and easy to tell apart.
pub const WORDS: [&str; 256] = [
    "acorn", "amber", "anchor", "apple", "arrow", "aspen", "autumn", "badge", "bagel", "bamboo",
    "banjo", "barley", "basil", "beach", "beacon", "bean", "berry", "birch", "biscuit", "blossom",
    "blue", "boat", "bolt", "bonnet", "book", "bottle", "branch", "brave", "breeze", "brick",
    "bridge", "brook", "brush", "bucket", "bunny", "butter", "cabin", "cactus", "camel", "candle",
    "canoe", "canyon", "carrot", "castle", "cedar", "chalk", "cherry", "chess", "chimney", "cider",
    "cinnamon", "circle", "citrus", "clay", "cliff", "clock", "cloud", "clover", "cobalt", "cocoa",
    "comet", "compass", "copper", "coral", "cotton", "crane", "crayon", "cricket", "crystal",
    "cup", "daisy", "dawn", "deer", "desert", "diamond", "dolphin", "dragon", "drum", "dune",
    "eagle", "earth", "echo", "elbow", "elm", "ember", "emerald", "engine", "fable", "falcon",
    "feather", "fern", "fiddle", "field", "fig", "flame", "flute", "forest", "fossil", "fox",
    "frost", "garden", "garlic", "gem", "ginger", "glacier", "globe", "goose", "grape", "gravel",
    "harbor", "hazel", "heron", "hill", "honey", "horizon", "iris", "island", "ivory", "jacket",
    "jade", "jasmine", "jelly", "jungle", "kayak", "kettle", "kite", "koala", "ladder", "lagoon",
    "lake", "lantern", "lemon", "lily", "lime", "linen", "lion", "lizard", "lobster", "lotus",
    "lunar", "magnet", "mango", "maple", "marble", "meadow", "melon", "mint", "mirror", "moon",
    "moss", "mountain", "muffin", "nectar", "needle", "nest", "noodle", "north", "nutmeg", "oak",
    "oasis", "ocean", "olive", "onion", "orange", "orbit", "orchid", "otter", "owl", "paddle",
    "panda", "paper", "parrot", "peach", "pearl", "pebble", "pepper", "piano", "pickle", "pigeon",
    "pillow", "pine", "planet", "plum", "pocket", "pond", "poppy", "potato", "prism", "pumpkin",
    "puzzle", "quartz", "quill", "rabbit", "radish", "rain", "raven", "reef", "ribbon", "river",
    "robin", "rocket", "rose", "ruby", "saddle", "saffron", "sail", "salmon", "sand", "sapphire",
    "scarf", "seal", "shell", "silver", "sky", "sleet", "smoke", "snow", "sparrow", "spice",
    "spoon", "spruce", "squash", "star", "stone", "storm", "straw", "stream", "sugar", "summit",
    "sun", "swan", "table", "tea", "thistle", "thunder", "tiger", "timber", "toast", "tomato",
    "topaz", "tulip", "tunnel", "turtle", "valley", "velvet", "violet", "walnut", "walrus",
    "water", "wave", "whale", "wheat", "willow", "window", "winter", "wolf",
];

/// A new six-word code from the OS's random source.
pub fn new_code() -> String {
    let mut rng = rand::rngs::OsRng;
    (0..6)
        .map(|_| WORDS[rng.gen_range(0..WORDS.len())])
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn words_are_unique_lowercase_letters() {
        let set: HashSet<_> = WORDS.iter().collect();
        assert_eq!(set.len(), 256);
        assert!(
            WORDS
                .iter()
                .all(|w| w.chars().all(|c| c.is_ascii_lowercase()))
        );
    }

    #[test]
    fn codes_are_six_known_words_and_differ() {
        let a = new_code();
        let b = new_code();
        assert_eq!(a.split(' ').count(), 6);
        assert!(a.split(' ').all(|w| WORDS.contains(&w)));
        assert_ne!(a, b);
        assert_eq!(
            oxide_core::chrome::clean_pairing_code(&a.to_uppercase()),
            Some(a)
        );
    }
}
