//! The openSUSE palette, shared by the TUI and GUI. Nothing else is used:
//! shades are blends of these colours, the way ArtCraft derives its shades
//! from its ink colour.

pub type Rgb = [u8; 3];

pub const GEEKO_GREEN: Rgb = [0x42, 0xcd, 0x42];
pub const YARROW_YELLOW: Rgb = [0xd4, 0xcb, 0x1b];
pub const ORANGE: Rgb = [0xf6, 0x89, 0x46];
pub const RADISH_RED: Rgb = [0xff, 0x5b, 0x45];
pub const PLUM_PURPLE: Rgb = [0xa4, 0x98, 0xff];
pub const BUTTERFLY_BLUE: Rgb = [0x00, 0xc8, 0xff];
pub const TURQUOISE_TEAL: Rgb = [0x20, 0xca, 0xa3];
pub const BAGEL_BEIGE: Rgb = [0xff, 0xf8, 0xee];
pub const GABBRO_GRAY: Rgb = [0xb8, 0xae, 0xab];
pub const MAPLE_MAROON: Rgb = [0x30, 0x1a, 0x14];

/// Module number tags cycle through these, like ArtCraft's per-app colours.
pub const ACCENTS: [Rgb; 7] = [BUTTERFLY_BLUE, ORANGE, PLUM_PURPLE, RADISH_RED, TURQUOISE_TEAL, GEEKO_GREEN, YARROW_YELLOW];

pub fn accent(index: usize) -> Rgb {
    ACCENTS[index % ACCENTS.len()]
}

/// `top` laid over `base` at `alpha` (0–1).
pub fn blend(base: Rgb, top: Rgb, alpha: f32) -> Rgb {
    let mix = |b: u8, t: u8| (b as f32 + (t as f32 - b as f32) * alpha).round() as u8;
    [mix(base[0], top[0]), mix(base[1], top[1]), mix(base[2], top[2])]
}

/// Colour roles for one appearance, by ArtCraft's token names.
#[derive(Clone, Copy, Debug)]
pub struct Tokens {
    pub dark: bool,
    pub bg: Rgb,
    pub sunken: Rgb,
    pub ink: Rgb,
    pub ink_strong: Rgb,
    pub muted: Rgb,
    pub faint: Rgb,
    pub line: Rgb,
    pub line_strong: Rgb,
    pub accent: Rgb,
    pub invert_bg: Rgb,
    pub invert_fg: Rgb,
    pub good: Rgb,
    pub warn: Rgb,
    pub bad: Rgb,
}

impl Tokens {
    pub fn dark() -> Self {
        Self {
            dark: true,
            bg: MAPLE_MAROON,
            sunken: blend(MAPLE_MAROON, BAGEL_BEIGE, 0.06),
            ink: BAGEL_BEIGE,
            ink_strong: BAGEL_BEIGE,
            muted: GABBRO_GRAY,
            faint: blend(MAPLE_MAROON, GABBRO_GRAY, 0.62),
            line: blend(MAPLE_MAROON, BAGEL_BEIGE, 0.16),
            line_strong: blend(MAPLE_MAROON, BAGEL_BEIGE, 0.40),
            accent: GEEKO_GREEN,
            invert_bg: BAGEL_BEIGE,
            invert_fg: MAPLE_MAROON,
            good: GEEKO_GREEN,
            warn: YARROW_YELLOW,
            bad: RADISH_RED,
        }
    }

    /// On beige the bright accents lack contrast as text, so text roles lean
    /// on maroon and the accents are blended towards it.
    pub fn light() -> Self {
        Self {
            dark: false,
            bg: BAGEL_BEIGE,
            sunken: blend(BAGEL_BEIGE, MAPLE_MAROON, 0.05),
            ink: MAPLE_MAROON,
            ink_strong: MAPLE_MAROON,
            muted: blend(BAGEL_BEIGE, MAPLE_MAROON, 0.70),
            faint: blend(BAGEL_BEIGE, MAPLE_MAROON, 0.48),
            line: blend(BAGEL_BEIGE, MAPLE_MAROON, 0.16),
            line_strong: blend(BAGEL_BEIGE, MAPLE_MAROON, 0.40),
            accent: blend(GEEKO_GREEN, MAPLE_MAROON, 0.35),
            invert_bg: MAPLE_MAROON,
            invert_fg: BAGEL_BEIGE,
            good: blend(GEEKO_GREEN, MAPLE_MAROON, 0.40),
            warn: blend(YARROW_YELLOW, MAPLE_MAROON, 0.45),
            bad: blend(RADISH_RED, MAPLE_MAROON, 0.25),
        }
    }

    pub fn tone(&self, tone: crate::Tone) -> Rgb {
        match tone {
            crate::Tone::Good => self.good,
            crate::Tone::Warn => self.warn,
            crate::Tone::Bad => self.bad,
            crate::Tone::Faint => self.faint,
            crate::Tone::Plain => self.ink,
        }
    }

    /// A module's accent, readable on this appearance's background.
    pub fn module_accent(&self, index: usize) -> Rgb {
        if self.dark {
            accent(index)
        } else {
            blend(accent(index), MAPLE_MAROON, 0.30)
        }
    }

    pub fn risk(&self, risk: yaist_core::Risk) -> Rgb {
        match risk {
            yaist_core::Risk::Read => self.muted,
            yaist_core::Risk::Change => self.warn,
            yaist_core::Risk::Dangerous => self.bad,
        }
    }
}
