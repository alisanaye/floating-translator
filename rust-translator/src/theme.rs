#[derive(Clone, Copy)]
pub struct Theme {
    pub bg: u32,
    pub bar_bg: u32,
    pub divider: u32,
    pub card_bg: u32,
    pub card_border: u32,
    pub hover_bg: u32,
    pub btn_bg: u32,
    pub border: u32,
    pub fg: u32,
    pub fg_muted: u32,
    pub fg_dim: u32,
    pub accent: u32,
    pub accent_hover: u32,
    pub accent_fg: u32,
    pub accent_green: u32,
    pub accent_amber: u32,
    pub accent_red: u32,
    pub err_fg: u32,
    pub key_fg: u32,
    pub select_bg: u32,
    pub switch_off: u32,
    pub knob: u32,
    pub close_hover: u32,
}

impl Theme {
    pub fn dark() -> Self {
        Self {
            bg: 0xFF14161A,
            bar_bg: 0xFF1D2128,
            divider: 0xFF232832,
            card_bg: 0xFF1B1F26,
            card_border: 0xFF252B35,
            hover_bg: 0xFF282E38,
            btn_bg: 0xFF232833,
            border: 0xFF2C323D,
            fg: 0xFFEEF1F6,
            fg_muted: 0xFF8B94A5,
            fg_dim: 0xFF5F6875,
            accent: 0xFF5FA8E8,
            accent_hover: 0xFF78B9F0,
            accent_fg: 0xFF14161A,
            accent_green: 0xFF6BD17B,
            accent_amber: 0xFFE0A84A,
            accent_red: 0xFFE06C75,
            err_fg: 0xFFFF9A8A,
            key_fg: 0xFFB39DDB,
            select_bg: 0xFF3B6EA5,
            switch_off: 0xFF363D4A,
            knob: 0xFFEEF1F6,
            close_hover: 0xFF7A2C2C,
        }
    }

    pub fn light() -> Self {
        Self {
            bg: 0xFFF5F7FA,
            bar_bg: 0xFFE9EDF3,
            divider: 0xFFD5DCE6,
            card_bg: 0xFFFFFFFF,
            card_border: 0xFFD8DFE9,
            hover_bg: 0xFFDCE3EE,
            btn_bg: 0xFFE4E9F2,
            border: 0xFFC5CEDB,
            fg: 0xFF1A1F29,
            fg_muted: 0xFF556070,
            fg_dim: 0xFF7B8696,
            accent: 0xFF2563EB,
            accent_hover: 0xFF3B82F6,
            accent_fg: 0xFFFFFFFF,
            accent_green: 0xFF16A34A,
            accent_amber: 0xFFD97706,
            accent_red: 0xFFDC2626,
            err_fg: 0xFFDC2626,
            key_fg: 0xFF7C3AED,
            select_bg: 0xFFBFDBFE,
            switch_off: 0xFFCBD5E1,
            knob: 0xFFFFFFFF,
            close_hover: 0xFFDC2626,
        }
    }

    pub fn for_name(name: &str) -> Self {
        if name == "light" {
            Self::light()
        } else {
            Self::dark()
        }
    }
}
