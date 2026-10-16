#![expect(
    missing_debug_implementations,
    reason = "EnumSet generates public serialization visitors."
)]

use enumset::EnumSetType;
use serde::{Deserialize, Serialize};
use std::fmt;

macro_rules! metadata_enum {
    ($(#[$attribute:meta])* $name:ident { $($variant:ident => $label:literal),+ $(,)? }) => {
        $(#[$attribute])*
        #[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
        pub enum $name {
            $(#[serde(rename = $label)] $variant),+
        }

        impl $name {
            /// The normalized display and serialization label.
            pub const fn as_str(self) -> &'static str {
                match self { $(Self::$variant => $label),+ }
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(self.as_str())
            }
        }
    };
}

metadata_enum! {
    /// The platform named by a release's service marker.
    StreamingService {
        Netflix => "Netflix",
        Amazon => "Amazon",
        ITunes => "iTunes",
        MoviesAnywhere => "Movies Anywhere",
        AppleTvPlus => "Apple TV+",
        DisneyPlus => "Disney+",
        HboMax => "HBO Max",
        Max => "Max",
        Hulu => "Hulu",
        Peacock => "Peacock",
        ParamountPlus => "Paramount+",
        Crunchyroll => "Crunchyroll",
    }
}

metadata_enum! {
    /// Edition and presentation qualifiers explicitly present in a release name.
    #[derive(EnumSetType)]
    #[enumset(no_super_impls, serialize_repr = "list")]
    Edition {
        Limited => "Limited",
        Unrated => "Unrated",
        Extended => "Extended",
        DirectorsCut => "Director's Cut",
        Theatrical => "Theatrical",
        Uncut => "Uncut",
        Uncensored => "Uncensored",
        Remastered => "Remastered",
        Restored => "Restored",
        Criterion => "Criterion",
        Imax => "IMAX",
        Special => "Special",
        Ultimate => "Ultimate",
        Collector => "Collector",
        Deluxe => "Deluxe",
        Anniversary => "Anniversary",
        AlternativeCut => "Alternative Cut",
        Festival => "Festival",
    }
}

metadata_enum! {
    /// Release status and packaging markers.
    #[derive(EnumSetType)]
    #[enumset(no_super_impls, serialize_repr = "list")]
    ReleaseFlag {
        Proper => "Proper",
        Repack => "Repack",
        Real => "Real",
        Internal => "Internal",
        ReadNfo => "Read NFO",
        NfoFix => "NFO Fix",
        ProofFix => "Proof Fix",
        Remux => "Remux",
        Hybrid => "Hybrid",
        Complete => "Complete",
        Subbed => "Subbed",
    }
}

metadata_enum! {
    /// A profile qualifying an audio codec.
    #[derive(EnumSetType)]
    #[enumset(no_super_impls, serialize_repr = "list")]
    AudioProfile {
        MasterAudio => "Master Audio",
        HighResolutionAudio => "High Resolution Audio",
        ExtendedSurround => "Extended Surround",
        LowComplexity => "Low Complexity",
        HighEfficiency => "High Efficiency",
    }
}
