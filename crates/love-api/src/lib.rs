// Derived from balatro-port-tui (Apache-2.0). Modified by Producdevity; see NOTICE.

pub mod audio;
mod card_cache;
#[cfg(any(feature = "native-sampler", feature = "layer-pairs"))]
mod card_layers;
pub mod event;
pub mod filesystem;
mod font;
mod game_source;
pub mod graphics;
pub mod keyboard;
pub mod lua_util;
mod miyoo;
#[cfg(all(feature = "native-sampler", target_os = "linux", target_arch = "arm"))]
pub mod native_sampler;
mod occlusion;
mod render_queue;
#[cfg(feature = "queue-profile")]
mod render_waits;
mod shader_inputs;
pub mod state;
pub mod stubs;
pub mod system;
mod text_cache;
mod text_value;
pub mod timer;
pub mod window;

use mlua::prelude::*;
use std::sync::Arc;

use state::SharedState;

pub struct LoveRuntime {
    pub lua: Lua,
    pub state: Arc<SharedState>,
}

impl LoveRuntime {
    pub fn new(state: Arc<SharedState>) -> anyhow::Result<Self> {
        let lua = Lua::new();

        let rt = LoveRuntime { lua, state };
        rt.setup_love_table()?;
        rt.setup_require()?;
        rt.setup_preloads()?;
        rt.override_print()?;
        rt.verify_bit_lib();
        Ok(rt)
    }

    fn setup_love_table(&self) -> anyhow::Result<()> {
        let lua = &self.lua;
        let love = lua.create_table()?;

        // Set love table globally first so sub-module registration can access it
        lua.globals().set("love", love.clone())?;

        audio::register(lua, &love, Arc::clone(&self.state))?;
        filesystem::register(lua, &love, Arc::clone(&self.state))?;
        timer::register(lua, &love, Arc::clone(&self.state))?;
        window::register(lua, &love, Arc::clone(&self.state))?;
        graphics::register(lua, &love, Arc::clone(&self.state))?;
        keyboard::register(lua, &love, Arc::clone(&self.state))?;
        event::register(lua, &love, Arc::clone(&self.state))?;
        system::register(lua, &love, Arc::clone(&self.state))?;
        stubs::register(lua, &love, Arc::clone(&self.state))?;

        lua.globals().set("love", love)?;

        Ok(())
    }

    fn setup_require(&self) -> anyhow::Result<()> {
        filesystem::setup_require(&self.lua, Arc::clone(&self.state))?;
        Ok(())
    }

    fn setup_preloads(&self) -> anyhow::Result<()> {
        filesystem::setup_preloads(&self.lua)?;
        Ok(())
    }

    /// Suppress game print calls while the terminal owns stdout and stderr.
    fn override_print(&self) -> anyhow::Result<()> {
        let print_fn = self.lua.create_function(|_, _args: LuaMultiValue| Ok(()))?;
        self.lua.globals().set("print", print_fn)?;
        Ok(())
    }

    fn verify_bit_lib(&self) {
        // LuaJIT provides 'bit' natively; verify it's accessible
        if let Err(e) = self.lua.load("require 'bit'").exec() {
            eprintln!("[WARN] 'bit' library not available: {}", e);
        }
    }
}
