use external_cas_assets::PackageIndex;
use ruffle_core::{
    backend::ui::*,
    font::{FontFileData, FontQuery},
};
use url::Url;
pub struct Ui {
    pub fonts: Vec<(String, Vec<u8>)>,
    pub audit: super::Audit,
    pub source_pin: Option<external_cas_fidelity::installed::SourcePin>,
    pub config_pin: Option<external_cas_fidelity::installed::SourcePin>,
}
impl Ui {
    pub fn load(index: &PackageIndex, audit: super::Audit) -> Result<Self, String> {
        let source = external_cas_fidelity::installed::load_movie(index, "gfxfontlib")?;
        let m = source.inspect()?;
        let mut fonts = Vec::new();
        for h in &m.compacted_font_headers {
            let matches = m
                .symbol_classes
                .iter()
                .filter(|s| s.id == h.id)
                .collect::<Vec<_>>();
            if matches.len() != 1 {
                return Err("ambiguous source font alias".into());
            }
            let f = external_cas_fidelity::font::CompactedFont::open(&m, h)
                .map_err(|e| e.to_string())?;
            let converted = external_cas_fidelity::truetype::convert(&f)?;
            fonts.push((matches[0].name.clone(), converted.bytes));
        }
        let config = external_cas_fidelity::font_config::load_font_config(index)?;
        for from in config.config.remaps.keys() {
            let target = config.config.canonical_alias(from)?;
            let matches = fonts
                .iter()
                .filter(|(name, _)| name == target)
                .collect::<Vec<_>>();
            if matches.len() != 1 {
                return Err(format!(
                    "source font remap {from}->{target} has {} available source aliases",
                    matches.len()
                ));
            }
            fonts.push((from.clone(), matches[0].1.clone()));
        }
        audit.push(format!("source-fontconfig: {} aliases/{} explicit legacy remaps; multi-face glyph fallback and alias metrics remain unqualified",config.config.aliases.len(),config.config.remaps.len()));
        Ok(Self {
            fonts,
            audit,
            source_pin: Some(source.pin),
            config_pin: Some(config.pin),
        })
    }
}
impl UiBackend for Ui {
    fn mouse_visible(&self) -> bool {
        true
    }
    fn set_mouse_visible(&mut self, _: bool) {}
    fn set_mouse_cursor(&mut self, _: MouseCursor) {}
    fn clipboard_content(&mut self) -> String {
        String::new()
    }
    fn set_clipboard_content(&mut self, _: String) {}
    fn set_fullscreen(&mut self, _: bool) -> Result<(), FullscreenError> {
        Err("isolated offscreen source test".into())
    }
    fn display_root_movie_download_failed_message(&self, _: bool, m: String) {
        self.audit.push(format!("root-download-error:{m}"));
    }
    fn message(&self, m: &str) {
        self.audit.push(format!("ui-message:{m}"));
    }
    fn open_virtual_keyboard(&self) {}
    fn close_virtual_keyboard(&self) {}
    fn language(&self) -> LanguageIdentifier {
        US_ENGLISH.clone()
    }
    fn display_unsupported_video(&self, u: Url) {
        self.audit.push(format!("video-denied:{u}"));
    }
    fn load_device_font(&self, q: &FontQuery, register: &mut dyn FnMut(FontDefinition)) {
        let matches = self
            .fonts
            .iter()
            .filter(|(name, _)| name.eq_ignore_ascii_case(&q.name))
            .collect::<Vec<_>>();
        if matches.len() == 1 {
            let (name, bytes) = matches[0];
            self.audit.push(format!(
                "source-font-alias:{}->{}; logical-style={}/{}",
                q.name, name, q.is_bold, q.is_italic
            ));
            register(FontDefinition::FontFile {
                name: q.name.clone(),
                is_bold: q.is_bold,
                is_italic: q.is_italic,
                data: FontFileData::new(bytes.clone()),
                index: 0,
            });
        } else {
            self.audit
                .push(format!("font-unresolved:{}; no system substitute", q.name));
        }
    }
    fn sort_device_fonts(
        &self,
        _: &FontQuery,
        _: &mut dyn FnMut(FontDefinition),
    ) -> Vec<FontQuery> {
        Vec::new()
    }
    fn display_file_open_dialog(&mut self, _: Vec<FileFilter>) -> Option<DialogResultFuture> {
        None
    }
    fn display_file_open_dialog_multiple(
        &mut self,
        _: Vec<FileFilter>,
    ) -> Option<MultiDialogResultFuture> {
        None
    }
    fn display_file_save_dialog(&mut self, _: String, _: String) -> Option<DialogResultFuture> {
        None
    }
    fn close_file_dialog(&mut self) {}
}
