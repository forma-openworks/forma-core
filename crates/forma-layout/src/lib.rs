//! Составной интерфейс программ Forma в духе Blender.
//!
//! Окно делится на области; в каждой области — один редактор, вид которого выбирается в меню
//! слева в заголовке. Области делят, объединяют и меняют местами жестами за уголки и из меню
//! границ, тянут за границы, разворачивают, переносят за заголовок в другие места и окна ОС.

mod dock;
pub mod tree;

pub use dock::{DockState, DockStyle, EditorKind, EditorViewer, FloatWindow, KindGroup, Place, SurfaceKey};
pub use tree::{Area, AreaId, Node, SplitDir, Surface, Zone};
