use crate::{AudioProfile, Edition, ParsedMedia, ReleaseFlag, StreamingService};

#[derive(Clone, Copy, Debug)]
pub(crate) enum Attribute {
    Service(StreamingService),
    Edition(Edition),
    Flag(ReleaseFlag),
    AudioProfile(AudioProfile),
}

impl Attribute {
    pub(crate) fn apply(self, media: &mut ParsedMedia) {
        match self {
            Self::Service(value) => {
                media.streaming_service.get_or_insert(value);
            }
            Self::Edition(value) => {
                media.edition.insert(value);
            }
            Self::Flag(value) => {
                media.release_flags.insert(value);
            }
            Self::AudioProfile(value) => {
                media.audio_profile.insert(value);
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Phrase {
    Ma,
    Master,
    HighResolution,
    ExtendedSurround,
    LowComplexity,
    HighEfficiency,
    Director,
    Alternative,
    Read,
    Nfo,
    Proof,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VideoCodec {
    H264,
    H265,
    Av1,
    Vp9,
    Xvid,
    Divx,
}

impl VideoCodec {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::H264 => "H.264",
            Self::H265 => "H.265",
            Self::Av1 => "AV1",
            Self::Vp9 => "VP9",
            Self::Xvid => "Xvid",
            Self::Divx => "DivX",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Word {
    Technical,
    VideoCodec(VideoCodec),
    VideoEncoder(VideoCodec),
    Phrase(Phrase),
    Service(StreamingService),
    ServiceBoundary(StreamingService),
    Edition(Edition),
    EditionBoundary(Edition),
    Flag(ReleaseFlag),
    FlagBoundary(ReleaseFlag),
}

impl Word {
    pub(crate) fn boundary(self) -> bool {
        matches!(
            self,
            Self::Technical
                | Self::VideoCodec(_)
                | Self::VideoEncoder(_)
                | Self::ServiceBoundary(_)
                | Self::EditionBoundary(_)
                | Self::FlagBoundary(_)
        )
    }

    pub(crate) fn attribute(self) -> Option<Attribute> {
        Some(match self {
            Self::Technical | Self::VideoCodec(_) | Self::VideoEncoder(_) | Self::Phrase(_) => {
                return None;
            }
            Self::Service(value) | Self::ServiceBoundary(value) => Attribute::Service(value),
            Self::Edition(value) | Self::EditionBoundary(value) => Attribute::Edition(value),
            Self::Flag(value) | Self::FlagBoundary(value) => Attribute::Flag(value),
        })
    }

    pub(crate) fn is_edition(self) -> bool {
        matches!(self, Self::Edition(_) | Self::EditionBoundary(_))
    }
}

pub(crate) fn classify(text: &str) -> Option<Word> {
    if text.get(..3).is_some_and(|prefix| {
        prefix.eq_ignore_ascii_case("dd+") || prefix.eq_ignore_ascii_case("ddp")
    }) {
        return Some(Word::Technical);
    }
    let mut buffer = [0u8; 16];
    let key = buffer.get_mut(..text.len())?;
    for (output, input) in key.iter_mut().zip(text.bytes()) {
        *output = input.to_ascii_lowercase();
    }
    word(key, text)
}

#[inline]
pub(crate) fn word(key: &[u8], original: &str) -> Option<Word> {
    use Attribute::{Edition as E, Flag as F, Service as S};
    use Edition::*;
    use ReleaseFlag::*;
    use StreamingService::*;
    use VideoCodec as V;
    let (value, boundary) = match key {
        b"x264" => return Some(Word::VideoEncoder(V::H264)),
        b"h264" | b"avc" => return Some(Word::VideoCodec(V::H264)),
        b"x265" => return Some(Word::VideoEncoder(V::H265)),
        b"h265" | b"hevc" => return Some(Word::VideoCodec(V::H265)),
        b"av1" => return Some(Word::VideoCodec(V::Av1)),
        b"vp9" => return Some(Word::VideoCodec(V::Vp9)),
        b"xvid" => return Some(Word::VideoEncoder(V::Xvid)),
        b"divx" => return Some(Word::VideoEncoder(V::Divx)),
        b"ma" => return Some(Word::Phrase(Phrase::Ma)),
        b"master" => return Some(Word::Phrase(Phrase::Master)),
        b"hra" | b"hr" => return Some(Word::Phrase(Phrase::HighResolution)),
        b"es" => return Some(Word::Phrase(Phrase::ExtendedSurround)),
        b"lc" => return Some(Word::Phrase(Phrase::LowComplexity)),
        b"he" => return Some(Word::Phrase(Phrase::HighEfficiency)),
        b"director" | b"directors" => return Some(Word::Phrase(Phrase::Director)),
        b"alternative" | b"alternate" => return Some(Word::Phrase(Phrase::Alternative)),
        b"read" => return Some(Word::Phrase(Phrase::Read)),
        b"nfo" => return Some(Word::Phrase(Phrase::Nfo)),
        b"proof" => return Some(Word::Phrase(Phrase::Proof)),
        b"480p" | b"480i" | b"576p" | b"576i" | b"720p" | b"1080p" | b"1080i" | b"1440p"
        | b"2160p" | b"4320p" | b"4k" | b"8k" | b"bluray" | b"brrip" | b"bdrip" | b"bdmux"
        | b"brmux" | b"bdripmux" | b"brripmux" | b"bd" | b"web" | b"webdl" | b"webrip"
        | b"hdtv" | b"pdtv" | b"sdtv" | b"tvrip" | b"hdrip" | b"hdtvrip" | b"hdcam" | b"camrip"
        | b"hdlight" | b"mhd" | b"dvdrip" | b"dvdr" | b"dvdscr" | b"vhs" | b"pal" | b"ntsc"
        | b"dvd" | b"hdr" | b"hdr10" | b"hdr10plus" | b"dv" | b"dovi" | b"dolbyvision" | b"sdr"
        | b"atmos" | b"truehd" | b"flac" | b"aac" | b"opus" | b"dts" | b"dd" | b"ddp" | b"ac3"
        | b"ac3d" | b"eac3" | b"dual" | b"convert" | b"10bit" | b"8bit" => {
            return Some(Word::Technical);
        }
        b"nf" => (S(Netflix), true),
        b"netflix" | b"nflx" => (S(Netflix), false),
        b"amzn" => (S(Amazon), true),
        b"amazon" => (S(Amazon), false),
        b"atvp" => (S(AppleTvPlus), true),
        b"dsnp" => (S(DisneyPlus), true),
        b"hmax" => (S(HboMax), true),
        b"hulu" => (S(Hulu), true),
        b"cr" => (S(Crunchyroll), true),
        b"crunchyroll" => (S(Crunchyroll), false),
        b"pcok" | b"peacock" => (S(Peacock), false),
        b"pmtp" | b"paramountplus" => (S(ParamountPlus), false),
        b"itunes" | b"itn" => (S(ITunes), false),
        b"it" if original == "iT" => (S(ITunes), false),
        b"max" if original == "MAX" => (S(Max), false),
        b"limited" => (E(Limited), true),
        b"unrated" => (E(Unrated), true),
        b"extended" => (E(Extended), true),
        b"directorscut" => (E(DirectorsCut), true),
        b"uncut" => (E(Uncut), true),
        b"remastered" => (E(Remastered), true),
        b"imax" => (E(Imax), true),
        b"remaster" => (E(Remastered), false),
        b"restored" | b"restore" => (E(Restored), false),
        b"uncensored" => (E(Uncensored), false),
        b"theatrical" => (E(Theatrical), false),
        b"criterion" => (E(Criterion), false),
        b"special" => (E(Special), false),
        b"ultimate" => (E(Ultimate), false),
        b"collector" | b"collectors" => (E(Collector), false),
        b"deluxe" => (E(Deluxe), false),
        b"anniversary" => (E(Anniversary), false),
        b"festival" => (E(Festival), false),
        b"proper" => (F(Proper), true),
        b"repack" => (F(Repack), true),
        b"nfofix" => (F(NfoFix), true),
        b"remux" | b"bdremux" | b"uhdremux" => (F(Remux), true),
        b"subbed" => (F(Subbed), true),
        b"real" => (F(Real), false),
        b"internal" | b"int" => (F(Internal), false),
        b"readnfo" => (F(ReadNfo), false),
        b"prooffix" => (F(ProofFix), false),
        b"hybrid" => (F(Hybrid), false),
        b"complete" => (F(Complete), false),
        _ => return None,
    };
    Some(match (value, boundary) {
        (S(value), true) => Word::ServiceBoundary(value),
        (S(value), false) => Word::Service(value),
        (E(value), true) => Word::EditionBoundary(value),
        (E(value), false) => Word::Edition(value),
        (F(value), true) => Word::FlagBoundary(value),
        (F(value), false) => Word::Flag(value),
        (Attribute::AudioProfile(_), _) => unreachable!(),
    })
}
