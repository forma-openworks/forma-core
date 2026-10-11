//! Составной интерфейс Forma Mycelium в духе Blender.
//!
//! Окно делится на области; у каждой области — вкладки с редакторами. Области можно
//! делить, тянуть за границы, разворачивать, а вкладки — перетаскивать между областями,
//! выносить в отдельные окна ОС и возвращать обратно.

mod dock;
pub mod tree;

pub use dock::{DockState, DockStyle, FloatWindow, SurfaceKey, TabViewer};
pub use tree::{Area, AreaId, Node, SplitDir, Surface, Zone};
