//! Retained primitives. QML geometry rules are explicit, without an implicit layout pass.
use crate::foundations::{centered, BASE_FONT};
use hyprv_ui::{
    animation::Color,
    geometry::Rect,
    scene::{Border, Content, Fill, Node, Text},
    skia::{self as sk},
};
#[derive(Clone, Debug, PartialEq)]
pub struct FontSpec {
    pub family: String,
    pub pixel_size: f32,
    pub weight: i32,
}
impl Default for FontSpec {
    fn default() -> Self {
        Self {
            family: BASE_FONT.into(),
            pixel_size: 16.,
            weight: 700,
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TextMetrics {
    pub width: f32,
    pub height: f32,
    pub advance: f32,
}
#[derive(Clone, Debug)]
pub struct Label {
    pub render_type: hyprv_ui::scene::RenderType,
    pub text: String,
    pub font: FontSpec,
    pub color: Color,
    pub elide: bool,
    pub max_lines: Option<usize>,
    pub letter_spacing: f32,
    pub align: sk::textlayout::TextAlign,
}
impl Label {
    pub fn new(text: impl Into<String>, font: FontSpec, color: Color) -> Self {
        Self {
            render_type: hyprv_ui::scene::RenderType::NativeRendering,
            text: text.into(),
            font,
            color,
            elide: true,
            max_lines: Some(1),
            letter_spacing: 0.,
            align: sk::textlayout::TextAlign::Left,
        }
    }
    pub fn measure(&self) -> TextMetrics {
        let spec = Text {
            families: vec![self.font.family.clone(), "HarmonyOS Sans SC".into()],
            size: self.font.pixel_size.trunc(),
            weight: self.font.weight.min(800),
            letter_spacing: self.letter_spacing,
            text: self.text.clone(),
            render_type: self.render_type,
            ..Default::default()
        };
        hyprv_ui::text::measure(&spec)
            .map(|m| TextMetrics {
                width: m.implicit_width,
                height: m.implicit_height,
                advance: m.advance,
            })
            .unwrap_or_default()
    }
    pub fn node(&self, frame: Rect) -> Node {
        Node::new(
            frame,
            Content::Text(Text {
                render_type: self.render_type,
                text: self.text.clone(),
                families: vec![self.font.family.clone(), "HarmonyOS Sans SC".into()],
                size: self.font.pixel_size.trunc(),
                weight: self.font.weight.min(800),
                color: self.color,
                letter_spacing: self.letter_spacing,
                max_lines: self.max_lines,
                elide: self.elide,
                align: self.align,
                subpixel: true,
                ..Default::default()
            }),
        )
    }
    pub fn centered_node(&self, parent: Rect) -> Node {
        let m = self.measure();
        self.node(centered(parent, (m.width, m.height)))
    }
}
pub fn rounded_rect(frame: Rect, radius: f32, fill: Color, border: Option<Border>) -> Node {
    let r = radius.min(frame.width / 2.).min(frame.height / 2.).max(0.);
    Node::new(
        frame,
        Content::Rect(hyprv_ui::scene::Rectangle {
            fill: Fill::Solid(fill),
            radii: [r; 4],
            border,
        }),
    )
}
pub fn glyph_icon(frame: Rect, glyph: impl Into<String>, px: f32, color: Color) -> Node {
    Label::new(
        glyph,
        FontSpec {
            pixel_size: px,
            ..Default::default()
        },
        color,
    )
    .node(frame)
}
pub fn clip_box(frame: Rect, children: Vec<Node>) -> Node {
    let mut node = Node::new(frame, Content::Group);
    node.clip = Some([0.; 4]);
    node.children = children;
    node
}

#[derive(Clone)]
pub enum ImageAsset {
    Raster(sk::Image),
    Svg(sk::svg::Dom),
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ImageFit {
    Fit,
    Crop,
    Stretch,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImageTicket(u64);
/// Bytes are read asynchronously; decode/completion is applied on the owning UI thread.
pub struct ImageIcon {
    pub asset: Option<ImageAsset>,
    pub intrinsic: (f32, f32),
    pub fit: ImageFit,
    pub tint: Option<Color>,
    pub source_size: Option<(u32, u32)>,
    ticket: u64,
    pub ready: hyprv_ui::reactive::Property<bool>,
}
impl Default for ImageIcon {
    fn default() -> Self {
        Self {
            asset: None,
            intrinsic: (1., 1.),
            fit: ImageFit::Fit,
            tint: None,
            source_size: None,
            ticket: 0,
            ready: hyprv_ui::reactive::Property::new(false),
        }
    }
}
impl ImageIcon {
    pub fn request(&mut self) -> ImageTicket {
        self.ticket += 1;
        self.asset = None;
        self.ready.set(false);
        ImageTicket(self.ticket)
    }
    pub fn complete(
        &mut self,
        ticket: ImageTicket,
        bytes: &[u8],
        svg: bool,
    ) -> Result<bool, String> {
        if ticket.0 != self.ticket {
            return Ok(false);
        }
        let asset = if svg {
            ImageAsset::Svg(
                sk::svg::Dom::from_bytes(bytes, sk::FontMgr::default())
                    .map_err(|e| e.to_string())?,
            )
        } else {
            ImageAsset::Raster(
                sk::Image::from_encoded(sk::Data::new_copy(bytes)).ok_or("image decode failed")?,
            )
        };
        if let ImageAsset::Raster(image) = &asset {
            self.intrinsic = (image.width() as f32, image.height() as f32);
        }
        self.asset = Some(asset);
        self.ready.set(true);
        Ok(true)
    }
    pub fn node(&self, frame: Rect) -> Node {
        let mut root = clip_box(frame, Vec::new());
        let Some(asset) = &self.asset else {
            return root;
        };
        let (iw, ih) = self.intrinsic;
        let scale = match self.fit {
            ImageFit::Fit => (frame.width / iw).min(frame.height / ih),
            ImageFit::Crop => (frame.width / iw).max(frame.height / ih),
            ImageFit::Stretch => 1.,
        };
        let (w, h) = if self.fit == ImageFit::Stretch {
            (frame.width, frame.height)
        } else {
            (iw * scale, ih * scale)
        };
        let rect = Rect::new((frame.width - w) / 2., (frame.height - h) / 2., w, h);
        let content = match asset {
            ImageAsset::Raster(image) => Content::Image(image.clone()),
            ImageAsset::Svg(dom) => Content::Svg(dom.clone()),
        };
        let mut child = Node::new(rect, content);
        child.effects.tint = self.tint;
        root.children.push(child);
        root
    }
    pub fn with_fallback(&self, frame: Rect, fallback: Node) -> Node {
        if self.ready.get() {
            self.node(frame)
        } else {
            let mut root = Node::new(frame, Content::Group);
            root.children.push(fallback);
            root
        }
    }
    pub fn rounded(
        &self,
        frame: Rect,
        radius: f32,
        placeholder: Color,
        fallback: Option<Node>,
    ) -> Node {
        let mut root = rounded_rect(frame, radius, placeholder, None);
        root.clip = Some([radius; 4]);
        if self.ready.get() {
            root.children
                .push(self.node(Rect::new(0., 0., frame.width, frame.height)));
        } else if let Some(fallback) = fallback {
            root.children.push(fallback);
        }
        root
    }
}
/// Retained Canvas-style vector paint, recorded only when inputs change.
pub fn canvas_item(frame: Rect, paint: impl FnOnce(&sk::Canvas)) -> Node {
    let mut recorder = sk::PictureRecorder::new();
    let canvas = recorder.begin_recording(sk::Rect::from_wh(frame.width, frame.height), false);
    paint(canvas);
    let picture = recorder
        .finish_recording_as_picture(None)
        .expect("recorded canvas");
    Node::new(frame, Content::Picture(picture))
}
/// Freedesktop icon file resolution as Qt's QIconLoader does it for the live shell
/// (theme chain, closest size, dash and unthemed fallbacks): see [`crate::icon_theme`].
/// `size` is in device pixels. No helper processes. Call once on source/theme changes.
pub fn resolve_icon(name: &str, size: u32) -> Option<std::path::PathBuf> {
    let direct = std::path::PathBuf::from(name);
    if direct.is_absolute() && direct.is_file() {
        return Some(direct);
    }
    crate::icon_theme::lookup(name, size, 1).map(|e| e.file)
}
type ImageJob = Box<dyn FnOnce() + Send>;
pub struct ImageLoader {
    send: Option<std::sync::mpsc::Sender<ImageJob>>,
    worker: Option<std::thread::JoinHandle<()>>,
}
impl ImageLoader {
    pub fn new() -> std::io::Result<Self> {
        let (send, receive) = std::sync::mpsc::channel::<ImageJob>();
        let worker = std::thread::Builder::new()
            .name("hyprv-image-io".into())
            .spawn(move || {
                while let Ok(job) = receive.recv() {
                    job();
                }
            })?;
        Ok(Self {
            send: Some(send),
            worker: Some(worker),
        })
    }
    pub fn read(
        &self,
        path: std::path::PathBuf,
        done: impl FnOnce(std::io::Result<Vec<u8>>) + Send + 'static,
    ) -> Result<(), String> {
        self.send
            .as_ref()
            .ok_or("loader stopped")?
            .send(Box::new(move || done(std::fs::read(path))))
            .map_err(|_| "loader stopped".into())
    }
}
impl Drop for ImageLoader {
    fn drop(&mut self) {
        self.send.take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
