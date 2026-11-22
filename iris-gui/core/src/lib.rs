//! The toolkit-free core of the IRIS front ends.
//!
//! Three layers, dependencies pointing down only (see
//! `clanker/plans/iris-newgui-plan.md`, "Layers"):
//!
//! - [`model`]: the domain. The emulator worker, the machine store, settings,
//!   jobs. Knows nothing of controllers or views.
//! - [`controller`]: one controller per task. `Action` in, [`controller::Effect`]
//!   out, plain read accessors. No toolkit, no raw threads.
//! - views: the egui app (`iris-gui/src`) and the native shells
//!   (`iris-gui/frontends/*`). They import only [`controller`].

pub mod controller;
pub mod model;
