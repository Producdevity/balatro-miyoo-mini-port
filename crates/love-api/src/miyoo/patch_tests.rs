use super::{
    patch_miyoo_script, MIYOO_CLASSIFIED_MOVEABLE_SCAN, MIYOO_MOVEABLE_MOVE,
    MIYOO_SMALL_SCREEN_PATCH,
};
use crate::render_queue::RenderQueue;
use crate::state::BackgroundCache;
use parking_lot::Mutex;
use sprite_to_text::pixel_buffer::PixelBuffer;
use std::sync::Arc;

#[test]
fn controller_dispatches_ordered_input_before_original_button_handling() {
    let mut source = br#"
function Controller:update(dt)
    self:set_HID_flags(self:update_axis(dt))
    original_input(self, dt)
end
"#
    .to_vec();
    super::patch_controller_input(&mut source);
    let once = source.clone();
    super::patch_controller_input(&mut source);
    assert_eq!(source, once);
    let lua = mlua::Lua::new();
    lua.load(
        r#"
        Controller = {set_HID_flags = function() end, update_axis = function() end}
        order = {}
        original_input = function()
            assert(order[1] == 0.02)
            order[2] = true
        end
    "#,
    )
    .exec()
    .unwrap();
    lua.load(&source).exec().unwrap();
    lua.load(
        r#"
        Controller.miyoo_dispatch = function(_, dt) order[1] = dt end
        Controller:update(0.02)
        assert(order[2])
    "#,
    )
    .exec()
    .unwrap();
}

#[test]
fn removed_ui_releases_only_its_owned_text_once() {
    let mut source = br#"
            UIElement = {}
            released, removed = 0, 0
function UIElement:remove()
            removed = removed + 1
end
            local text = {release = function() released = released + 1 end}
            local ui = setmetatable({config = {text_drawable = text}}, {__index = UIElement})
            ui:remove()
            assert(released == 1 and ui.config.text_drawable == nil)
            ui:remove()
            assert(released == 1 and removed == 2)
        "#
    .to_vec();
    patch_miyoo_script("engine/ui.lua", &mut source);
    mlua::Lua::new().load(&source).exec().unwrap();
}

#[test]
fn render_queue_flushes_draws_in_order() {
    let buffer = Arc::new(Mutex::new(PixelBuffer::new(1, 1)));
    let queue = RenderQueue::new(Arc::clone(&buffer)).unwrap();
    queue.submit(crate::render_queue::RenderJob::new(|buffer| {
        buffer.pixels[0] = 10
    }));
    queue.submit(crate::render_queue::RenderJob::new(|buffer| {
        buffer.pixels[0] += 5
    }));
    queue.flush();
    assert_eq!(buffer.lock().pixels[0], 15);
}

#[test]
fn miyoo_small_screen_patch_parses() {
    mlua::Lua::new()
        .load(MIYOO_SMALL_SCREEN_PATCH)
        .into_function()
        .expect("small-screen patch should parse");
}

#[test]
fn miyoo_globals_use_low_cost_render_settings() {
    let mut script =
            b"self.F_MUTE = false\nself.F_SOUND_THREAD = true\ntexture_scaling = 2\nshadows = 'On'\nself.TILE_H = 11.5\nself.MOVEABLES = {}"
                .to_vec();
    patch_miyoo_script("globals.lua", &mut script);
    let script = String::from_utf8(script).unwrap();
    assert!(script.contains("self.F_MUTE = os.getenv('BALATRO_AUDIO') ~= '1'"));
    assert!(script.contains("self.F_SOUND_THREAD = false"));
    assert!(script.contains("texture_scaling = 1"));
    assert!(script.contains("shadows = 'Off'"));
    assert!(script.contains("self.SVMM_HANDHELD_LAYOUT"));
    assert!(script.contains("self.SVMM_UPDATEABLES = {}"));
}

#[test]
fn miyoo_reflows_the_widescreen_room_for_a_handheld_display() {
    let mut resize = b"if w/h < G.window_prev.orig_ratio then\n\t\tG.TILESCALE = G.window_prev.orig_scale*w/G.window_prev.w\n\telse\n\t\tG.TILESCALE = G.window_prev.orig_scale*h/G.window_prev.h\n\tend\nif w/h < G.window_prev.orig_ratio then\n\t\t\tG.ROOM.T.x = G.ROOM_PADDING_W\n\t\t\tG.ROOM.T.y = (h/(G.TILESIZE*G.TILESCALE) - (G.ROOM.T.h+G.ROOM_PADDING_H))/2 + G.ROOM_PADDING_H/2\n\t\telse".to_vec();
    patch_miyoo_script("main.lua", &mut resize);
    let resize = String::from_utf8(resize).unwrap();
    assert!(resize.contains("G.TILESCALE = w/(G.TILESIZE*G.TILE_W)"));
    assert!(resize.contains("G.ROOM.T.x = 0"));
    assert!(resize.contains("G.ROOM.T.y = (h/(G.TILESIZE*G.TILESCALE) - G.ROOM.T.h)/2"));

    let mut game =
        b"config = {align=('cli'), offset = {x=-0.7,y=0},major = G.ROOM_ATTACH}".to_vec();
    patch_miyoo_script("game.lua", &mut game);
    let game = String::from_utf8(game).unwrap();
    assert!(game.contains("G.SVMM_HANDHELD_LAYOUT and 0 or -0.7"));

    let mut positions =
            b"G.hand.T.x = G.TILE_W - G.hand.T.w - 2.85\nG.hand.T.y = G.TILE_H - G.hand.T.h\nG.deck.T.x = G.TILE_W - G.deck.T.w - 0.5"
                .to_vec();
    patch_miyoo_script("functions/common_events.lua", &mut positions);
    let positions = String::from_utf8(positions).unwrap();
    assert!(positions.contains("G.SVMM_HANDHELD_LAYOUT and 2.35 or 2.85"));
    assert!(positions.contains("G.SVMM_HANDHELD_LAYOUT and 1.15 or 0"));
    assert!(positions.contains("G.SVMM_HANDHELD_LAYOUT and 0.1 or 0.5"));

    let mut hud = b"minh = 30, padding = 0.08".to_vec();
    patch_miyoo_script("functions/UI_definitions.lua", &mut hud);
    let hud = String::from_utf8(hud).unwrap();
    assert!(hud.contains("minh = G.SVMM_HANDHELD_LAYOUT and 0 or 30"));
}

#[test]
fn miyoo_keeps_the_game_sound_path_enabled() {
    let mut script = br#"
function modulate_sound(dt)
  --volume of the splash screen is set here
  return 'original sound path'
end
G = {F_MUTE = false}
"#
    .to_vec();
    patch_miyoo_script("functions/misc_functions.lua", &mut script);
    let lua = mlua::Lua::new();
    lua.load(&script).exec().unwrap();
    let value: String = lua
        .load("return modulate_sound(0.016)")
        .eval()
        .expect("sound update should execute");
    assert_eq!(value, "original sound path");
}

#[test]
fn miyoo_muted_sound_update_keeps_visual_score_intensity() {
    let mut script = br#"
function modulate_sound(dt)
  --volume of the splash screen is set here
  error('muted sound work should not run')
end
G = {
  F_MUTE = true,
  ARGS = {chip_flames = {real_intensity = 3, change = 2}},
  GAME = {
    current_round = {current_hand = {chips = 12, mult = 3}},
    blind = {chips = 100}
  }
}
modulate_sound(0.016)
"#
    .to_vec();
    patch_miyoo_script("functions/misc_functions.lua", &mut script);
    let lua = mlua::Lua::new();
    lua.load(&script).exec().unwrap();
    let values: (f64, f64) = lua
        .load("return G.ARGS.score_intensity.earned_score, G.ARGS.score_intensity.required_score")
        .eval()
        .expect("muted sound modulation should execute");
    assert_eq!(values, (36.0, 100.0));
}

#[test]
fn miyoo_exposes_trimmed_score_update_to_selective_jit() {
    let mut script = b"function Game:update(dt)\nend".to_vec();
    patch_miyoo_script("game.lua", &mut script);
    let script = String::from_utf8(script).unwrap();
    assert!(script.contains("SVMM_UPDATE_JIT_FUNCTIONS = {score_intensity = modulate_sound}"));
}

#[test]
fn unrelated_files_are_unchanged() {
    let mut script = b"texture_scaling = 2".to_vec();
    patch_miyoo_script("main.lua", &mut script);
    assert_eq!(script, b"texture_scaling = 2");
}

#[test]
fn miyoo_font_raster_size_preserves_layout_scale() {
    let mut script = b"render_scale = self.TILESIZE*10, FONTSCALE = 0.1, squish = 1\nrender_scale = self.TILESIZE*7, FONTSCALE = 0.12, squish = 1".to_vec();
    patch_miyoo_script("game.lua", &mut script);
    let script = String::from_utf8(script).unwrap();
    assert!(script.contains("self.TILESIZE*2.5, FONTSCALE = 0.4, LAYOUT_FONTSCALE = 0.1"));
    assert!(script.contains("self.TILESIZE*2, FONTSCALE = 0.42, LAYOUT_FONTSCALE = 0.12"));
}

#[test]
fn miyoo_dynatext_keeps_original_spacing_and_motion_scale() {
    let mut script = br#"
function DynaText:init(config)
    self.config = config
    self.shadow = config.shadow
    self.font = config.font or G.LANG.font
end
local tx = width + 2.7*(self.config.spacing or 0)*G.TILESCALE*self.font.FONTSCALE
local float = (self.font.FONTSCALE/G.TILESIZE)*2000*math.sin(t)
love.graphics.translate(self.config.spacing*self.font.FONTSCALE/G.TILESIZE, 0)
local x = 0.5*(letter.dims.x - letter.offset.x)*self.font.FONTSCALE/G.TILESIZE
local y = 0.5*(letter.dims.y - letter.offset.y)*self.font.FONTSCALE/G.TILESIZE
"#
    .to_vec();
    patch_miyoo_script("engine/text.lua", &mut script);
    let script = String::from_utf8(script).unwrap();

    assert!(script
        .contains("self.layout_font_scale = self.font.LAYOUT_FONTSCALE or self.font.FONTSCALE"));
    assert!(script.contains("G.TILESCALE*self.layout_font_scale"));
    assert!(script.contains("(self.layout_font_scale/G.TILESIZE)*2000"));
    assert!(script.contains("self.config.spacing*self.layout_font_scale/G.TILESIZE"));
    assert!(script
        .contains("letter.dims.x*self.font.FONTSCALE - letter.offset.x*self.layout_font_scale"));
    assert!(script
        .contains("letter.dims.y*self.font.FONTSCALE - letter.offset.y*self.layout_font_scale"));
}

#[test]
fn miyoo_dynatext_skips_only_constant_idle_labels() {
    let mut script = br#"
function DynaText:init(config)
    self.config = config
    self.shadow = config.shadow
    self.font = config.font or G.LANG.font
    self:update_text(true)
    if self.config.maxw then return end
end
function DynaText:update(dt)
    self:update_text()
    self:align_letters()
end
"#
    .to_vec();
    patch_miyoo_script("engine/text.lua", &mut script);
    let script = String::from_utf8(script).unwrap();

    assert!(script.contains("self._svmm_can_idle = #self.config.string == 1"));
    assert!(script.contains("first_string.ref_table"));
    assert!(script.contains("not self.config.float and not self.config.bump"));
    assert!(script.contains("not self.config.pop_in"));
    assert!(script.contains("not self.config.pop_out"));
    assert!(script.contains("not self.config.pulse"));
    assert!(script.contains("not self.config.quiver then return end"));
    mlua::Lua::new()
        .load(&script)
        .into_function()
        .expect("patched DynaText should parse");
}

#[test]
fn miyoo_draws_directly_to_the_screen() {
    let mut script = b"love.graphics.setCanvas{self.CANVAS}\n        love.graphics.draw(self.CANVAS, 0, 0)\nlove.graphics.setCanvas(G.AA_CANVAS)\n    if G.AA_CANVAS then \n        love.graphics.push()\n            love.graphics.scale(1/G.CANV_SCALE)\n            love.graphics.draw(G.AA_CANVAS, 0, 0)\n        love.graphics.pop()\n    end".to_vec();
    patch_miyoo_script("game.lua", &mut script);
    let script = String::from_utf8(script).unwrap();

    assert!(script.contains("love.graphics.setCanvas()"));
    assert!(!script.contains("love.graphics.setCanvas{self.CANVAS}"));
    assert!(!script.contains("love.graphics.draw(self.CANVAS, 0, 0)"));
    assert!(!script.contains("love.graphics.setCanvas(G.AA_CANVAS)"));
    assert!(!script.contains("love.graphics.draw(G.AA_CANVAS, 0, 0)"));
}

#[test]
#[ignore = "requires user-owned game archive in BALATRO_TEST_GAME"]
fn real_game_clears_the_screen_before_drawing() {
    let path = std::env::var("BALATRO_TEST_GAME").expect("set BALATRO_TEST_GAME");
    let source = crate::state::GameSource::from_path(std::path::Path::new(&path)).unwrap();
    let mut script = source.read_file("game.lua").unwrap();
    patch_miyoo_script("game.lua", &mut script);
    let script = String::from_utf8(script).unwrap();
    let start = script.find("function Game:draw()").unwrap();
    let end = start
        + script[start..]
            .find("love.graphics.clear(0,0,0,1)")
            .unwrap()
        + "love.graphics.clear(0,0,0,1)".len();
    let lua = mlua::Lua::new();
    lua.load(
        r#"
            Game = {}; G = {FRAMES = {DRAW = 0}, CANV_SCALE = 1}
            local target = 'old canvas'
            love = {graphics = {
                setCanvas = function(value) target = value end,
                push = function() end, scale = function() end, setShader = function() end,
                clear = function(r, g, b, a)
                    assert(target == nil and r == 0 and g == 0 and b == 0 and a == 1)
                    cleared = true
                end
            }}
            reset_drawhash = function() end; timer_checkpoint = function() end
        "#,
    )
    .exec()
    .unwrap();
    lua.load(format!("{}\nend\nGame:draw()", &script[start..end]))
        .exec()
        .unwrap();
    assert!(lua.globals().get::<bool>("cleared").unwrap());
}

#[test]
fn miyoo_removes_the_unused_final_shader_pass() {
    let mut script = b"love.graphics.pop()\n    \n    love.graphics.setCanvas(G.AA_CANVAS)\n    G.SHADERS['CRT']:send('time', 400)\n    love.graphics.draw(self.CANVAS, 0, 0)\n    timer_checkpoint('canvas', 'draw')".to_vec();
    patch_miyoo_script("game.lua", &mut script);
    let script = String::from_utf8(script).unwrap();
    assert!(!script.contains("G.SHADERS['CRT']:send"));
    assert!(!script.contains("love.graphics.draw(self.CANVAS"));
    assert!(script.contains("love.graphics.setShader()"));
    assert!(script.contains("timer_checkpoint('canvas', 'draw')"));
}

#[test]
#[ignore = "requires user-owned game archive in BALATRO_TEST_GAME"]
fn real_game_draws_foreground_panels_after_cards() {
    let path = std::env::var("BALATRO_TEST_GAME").expect("set BALATRO_TEST_GAME");
    let source = crate::state::GameSource::from_path(std::path::Path::new(&path)).unwrap();
    let mut script = source.read_file("game.lua").unwrap();
    patch_miyoo_script("game.lua", &mut script);
    let script = String::from_utf8(script).unwrap();
    let start = script
        .find("        timer_checkpoint('primatives', 'draw')")
        .unwrap();
    let end = start
        + script[start..]
            .find("        G.under_overlay = false")
            .unwrap();
    let lua = mlua::Lua::new();
    lua.load(
        r#"
        order = {}
        local function box(name, config)
            return {config=config or {}, translate_container=function() end,
                draw=function() order[#order+1] = name end}
        end
        local attention = box('attention')
        attention.attention_text = true
        self = {I={
            UIBOX={box('background'), box('blind', {draw_after_cards=true}), attention},
            CARDAREA={box('cards')}, CARD={box('loose card')}
        }, CONTROLLER={dragging={}, focused={}}}
        G = self
        love = {graphics={push=function() end, pop=function() end}}
        timer_checkpoint = function() end
        "#,
    )
    .exec()
    .unwrap();
    lua.load(&script[start..end]).exec().unwrap();
    let order: String = lua.load("return table.concat(order, ',')").eval().unwrap();
    assert_eq!(order, "background,cards,loose card,blind,attention");
}

#[test]
fn miyoo_moveables_skip_only_settled_movement() {
    let mut script = b"function test()\n    self.CALCING = nil\n    if self.role.role_type == 'Glued' then\n        return\n    elseif self.role.role_type == 'Minor' then\n        if false or\n            self.role.xy_bond == 'Weak' or \n            self.role.r_bond == 'Weak' then  \n            return\n        end\n    end\nend"
            .to_vec();
    patch_miyoo_script("engine/moveable.lua", &mut script);
    let script = String::from_utf8(script).unwrap();
    assert!(script.contains("self.role.role_type == 'Major' and self.STATIONARY"));
    assert!(MIYOO_MOVEABLE_MOVE.contains("not classified_active and role_type == 'Major'"));
    assert!(script.contains("not self.config.refresh_movement"));
    assert!(script.contains("states.drag.is or states.hover.is"));
    assert!(script.contains("math.abs(velocity.x) > 0.01"));
    assert!(!script.contains("self.role.xy_bond == 'Weak' or"));
    assert!(!script.lines().any(|line| line.starts_with('+')));
    mlua::Lua::new()
        .load(&script)
        .into_function()
        .expect("patched moveable script should parse");
}

#[test]
fn miyoo_minor_transform_matches_bond_rules() {
    let mut script = br#"
Moveable = {}
function Moveable:move_with_major(dt)
    error('original move_with_major was not replaced')
end
function Moveable:move_xy(dt)
    self.calls.xy = self.calls.xy + 1
    self.VT.x, self.VT.y = self.T.x + dt, self.T.y - dt
end
function Moveable:move_r(dt, velocity)
    self.calls.r = self.calls.r + 1
    self.VT.r = self.T.r - dt
end
function Moveable:move_scale(dt)
    self.calls.scale = self.calls.scale + 1
    self.VT.scale = self.T.scale + dt
end
function Moveable:move_wh(dt)
    self.calls.wh = self.calls.wh + 1
    self.VT.w, self.VT.h = self.T.w + dt, self.T.h - dt
end
function Moveable:move_juice(dt)
    self.calls.juice = self.calls.juice + 1
    self.juice = {r = 0.1, scale = 0.2}
end
function Moveable:get_major()
    return {major = self, offset = {x = 0.5, y = -0.25}}
end

G = {ROOM = {T = {w = 20}}}
local major = setmetatable({
    T = {x = 10, y = 20, w = 4, h = 6, r = 0, scale = 2},
    VT = {x = 11, y = 19, w = 5, h = 7, r = 0, scale = 3}
}, {__index = Moveable})

local function minor(bond)
    return setmetatable({
        role = {
            role_type = 'Minor', major = major, offset = {x = 1, y = 2},
            xy_bond = bond, r_bond = bond, scale_bond = bond, wh_bond = bond
        },
        T = {x = 0, y = 0, w = 2, h = 2, r = 0.25, scale = 0.5},
        VT = {x = 0, y = 0, w = 2, h = 2, r = 0, scale = 0.5},
        velocity = {x = 0, y = 0, r = 0, scale = 0},
        shadow_parrallax = {x = 0, y = 0},
        calls = {juice = 0, xy = 0, r = 0, scale = 0, wh = 0}
    }, {__index = Moveable})
end

local function result(item)
    return string.format(
        '%.4f,%.4f,%.4f,%.4f,%.4f,%.4f,%.4f;%d,%d,%d,%d,%d',
        item.T.x, item.T.y, item.VT.x, item.VT.y, item.VT.r,
        item.VT.scale, item.shadow_parrallax.x,
        item.calls.juice, item.calls.xy, item.calls.r,
        item.calls.scale, item.calls.wh)
end

local strong = minor('Strong')
strong:move_with_major(0.3)
local weak = minor('Weak')
weak:move_with_major(0.3)
return result(strong), result(weak), strong.VT.w, strong.VT.h,
    weak.VT.w, weak.VT.h
"#
    .to_vec();
    patch_miyoo_script("engine/moveable.lua", &mut script);
    let (strong, weak, strong_w, strong_h, weak_w, weak_h): (String, String, f64, f64, f64, f64) =
        mlua::Lua::new()
            .load(&script)
            .eval()
            .expect("optimized minor movement should execute");

    assert_eq!(
        strong,
        "11.5000,21.7500,12.2500,20.7500,0.3500,0.9500,0.3750;1,0,0,0,0"
    );
    assert_eq!(
        weak,
        "11.5000,21.7500,11.8000,21.4500,-0.0500,0.8000,0.3750;1,1,1,1,1"
    );
    assert!((strong_w - 2.5).abs() < f64::EPSILON);
    assert!((strong_h - 7.0 / 3.0).abs() < f64::EPSILON);
    assert!((weak_w - 2.3).abs() < f64::EPSILON);
    assert!((weak_h - 1.7).abs() < f64::EPSILON);
}

#[test]
fn miyoo_hot_object_lists_use_array_iteration() {
    let mut game = b"        for k, v in pairs(self.MOVEABLES) do\n            if v.FRAME.MOVE < G.FRAMES.MOVE then v:move(move_dt) end\n        end\n                    timer_checkpoint('move', 'update')\n        \n        for k, v in pairs(self.MOVEABLES) do\n            v:update(dt*self.SPEEDFACTOR)\n            v.states.collide.is = false\n        end\nfor k, v in pairs(self.I.NODE) do\nend\nfor k, v in pairs(self.I.UIBOX) do\nend"
            .to_vec();
    patch_miyoo_script("game.lua", &mut game);
    let game = String::from_utf8(game).unwrap();
    assert!(game.contains("local moveables = self.MOVEABLES"));
    assert!(game.contains("if role_type == 'Glued' then"));
    assert!(game.contains("local previous, offset = alignment.prev_offset, alignment.offset"));
    assert!(game.contains("item._svmm_glued_major == major"));
    assert!(game.contains("major.FRAME.MOVE ~= frame_move or not major.STATIONARY"));
    assert!(game.contains("for i = 1, #moveables do"));
    assert!(game.contains("local collisions = controller.collision_list"));
    assert!(game.contains("collisions[i].states.collide.is = false"));
    assert!(game.contains("dragging.target.states.collide.is = false"));
    assert!(game.contains("local updateables = self.SVMM_UPDATEABLES"));
    assert!(game.contains("item:move(dt, true)"));
    assert!(game.contains("while i <= #updateables do"));
    assert!(game.contains("item:update(update_dt)"));
    assert!(game.contains("if updateables[i] == item then i = i + 1 end"));
    assert!(game.contains("for i = 1, #self.I.NODE do"));
    assert!(game.contains("for i = 1, #self.I.UIBOX do"));
    assert!(!game.contains("pairs(self.MOVEABLES)"));

    let mut ui = b"function test()\n        for k, v in pairs(self.children) do\n            if not v.config.draw_layer and k ~= 'h_popup' and k~= 'alert' then \n                v:draw()\n            end\n        end\nend"
            .to_vec();
    patch_miyoo_script("engine/ui.lua", &mut ui);
    let ui = String::from_utf8(ui).unwrap();
    assert!(ui.contains("pairs(self.children)"));
    assert!(ui.contains("k ~= 'h_popup'"));
    mlua::Lua::new()
        .load(&ui)
        .into_function()
        .expect("patched UI script should parse");
}

#[test]
fn miyoo_tracks_only_moveables_that_need_updates() {
    let mut script = b"function Moveable:init()\n    table.insert(G.MOVEABLES, self)\n    if getmetatable(self) == Moveable then \n    end\nend\nfunction Moveable:remove()\n    for k, v in pairs(G.MOVEABLES) do\n    end\nend".to_vec();
    patch_miyoo_script("engine/moveable.lua", &mut script);
    let script = String::from_utf8(script).unwrap();
    assert!(script.contains("local needs_update = update ~= Node.update"));
    assert!(script.contains("kind == G.UIT.O"));
    assert!(script.contains("kind == G.UIT.T and config.ref_table and config.ref_value"));
    assert!(!script.contains("config.func or config.button or config.button_UIE"));
    assert!(script.contains("table.insert(G.SVMM_UPDATEABLES, self)"));
    assert!(script.contains("table.remove(G.SVMM_UPDATEABLES, i)"));
}

#[test]
fn miyoo_ui_updates_keep_callbacks_without_debug_tracking() {
    let mut script = br#"UIElement = {}
function UIElement:update(dt)
    G.ARGS.FUNC_TRACKER = G.ARGS.FUNC_TRACKER or {}
    if self.config.button_delay then
        self.config.button_temp = self.config.button or self.config.button_temp
        self.config.button = nil
        self.config.button_delay_progress = (G.TIMERS.REAL - self.config.button_delay_start)/self.config.button_delay
        if G.TIMERS.REAL >= self.config.button_delay_end then self.config.button_delay = nil end
    end
    if self.config.button_temp and not self.config.button_delay then self.config.button = self.config.button_temp end
    if self.button_clicked then self.button_clicked = nil end
    if self.config and self.config.func then
        G.ARGS.FUNC_TRACKER[self.config.func] = (G.ARGS.FUNC_TRACKER[self.config.func] or 0) + 1
        G.FUNCS[self.config.func](self)
    end
    if self.UIT == G.UIT.T then self:update_text() end
    if self.UIT == G.UIT.O then self:update_object() end
    Node.update(self, dt)
end
local calls = 0
G = {ARGS = {}, TIMERS = {REAL = 1}, UIT = {T = 1, O = 2}, FUNCS = {tick = function() calls = calls + 1 end}}
local element = {config = {func = 'tick'}, UIT = 3}
setmetatable(element, {__index = UIElement})
element:update(0.016)
return calls"#
            .to_vec();
    patch_miyoo_script("engine/ui.lua", &mut script);
    let script = String::from_utf8(script).unwrap();
    assert!(!script.contains("FUNC_TRACKER"));
    assert!(!script.contains("Node.update"));
    let calls: i64 = mlua::Lua::new()
        .load(&script)
        .eval()
        .expect("patched UI update should execute");
    assert_eq!(calls, 1);
}

#[test]
fn miyoo_pixelated_rects_reuse_native_polygon_data() {
    let mut script = b"function UIElement:draw_pixellated_rect(_type)\n    love.graphics.polygon((_type == 'line' or _type == 'line_emboss') and 'line' or \"fill\", self.pixellated_rect[_type].vertices)\nend"
            .to_vec();
    patch_miyoo_script("engine/ui.lua", &mut script);
    let script = String::from_utf8(script).unwrap();
    assert!(script.contains("polygon.native = polygon.native or love.graphics.newPolygonData"));
    assert!(script.contains("if _native_only then return polygon.native end"));
    assert!(script.contains("love.graphics.drawPolygonData"));
    assert!(!script.contains("love.graphics.polygon("));
    mlua::Lua::new()
        .load("UIElement = {}\nlove = {graphics = {}}\n".to_owned() + &script)
        .into_function()
        .expect("patched pixelated rectangle draw should parse");
}

#[test]
fn miyoo_disables_ui_shadows_when_objects_are_created() {
    let mut ui = b"function UIElement:init(config)\n    self.config = config or {}\n    if self.config and self.config.object then self.config.object.parent = self end\nend".to_vec();
    patch_miyoo_script("engine/ui.lua", &mut ui);
    let ui = String::from_utf8(ui).unwrap();
    assert!(ui.contains("self.config.shadow = nil"));
    assert!(ui.contains("self.config.emboss = nil"));
    assert!(ui.contains("self.config.line_emboss = nil"));

    let mut text = b"function DynaText:init(config)\n    self.config = config\n    self.shadow = config.shadow\nend".to_vec();
    patch_miyoo_script("engine/text.lua", &mut text);
    let text = String::from_utf8(text).unwrap();
    assert!(text.contains("self.config.shadow = false"));
    assert!(text.contains("self.shadow = false"));
}

#[test]
fn miyoo_ui_draw_keeps_interaction_without_shadow_branches() {
    let mut script = b"function UIElement:draw_self()\n    if self.config.shadow and G.SETTINGS.GRAPHICS.shadows == 'On' then return end\nend\nfunction UIElement:draw_pixellated_rect()\nend".to_vec();
    patch_miyoo_script("engine/ui.lua", &mut script);
    let script = String::from_utf8(script).unwrap();
    assert!(script.contains("config.force_focus or config.force_collision"));
    assert!(script.contains("config.object:draw()"));
    assert!(script.contains("if states.focus.is then"));
    assert!(script.contains("love.graphics._drawUIBatch"));
    assert!(script.contains("SVMM_flush_ui_draws()"));
    assert!(script.contains("SVMM_UI_JIT_FUNCTIONS"));
    assert!(script.contains("SVMM_plain_ui_kind"));
    assert!(script.contains("if not config.text_drawable then UIElement.update_text(self) end"));
    assert!(!script.contains("GRAPHICS.shadows"));
    assert!(!script.contains("draw_boundingrect"));
    mlua::Lua::new()
        .load("UIElement = {}\n".to_owned() + &script)
        .into_function()
        .expect("specialized UI draw should parse");

    let classifications: (i64, i64, i64) = mlua::Lua::new()
        .load(
            "UIElement = {}\n".to_owned()
                + &script
                + r#"
G = {UIT = {T = 1, B = 2, C = 3, R = 4, ROOT = 5}}
local box = {}
local states = {collide = {can = false}}
local text = {UIT = G.UIT.T, UIBox = box}
local panel = {UIT = G.UIT.C, UIBox = box}
local dynamic_parent = {config = {func = 'update'}, parent = nil}
local dynamic_text = {UIT = G.UIT.T, UIBox = box, parent = dynamic_parent}
return SVMM_UI_JIT_FUNCTIONS.plain_kind(text, {scale = 1}, states),
    SVMM_UI_JIT_FUNCTIONS.plain_kind(panel, {r = 0.2}, states),
    SVMM_UI_JIT_FUNCTIONS.plain_kind(dynamic_text, {scale = 1}, states)
"#,
        )
        .eval()
        .expect("plain UI classification should execute");
    assert_eq!(classifications, (1, 2, 0));

    let fallback: (f64, f64, f64, f64, i64) = mlua::Lua::new()
        .load(
            "UIElement = {}\n".to_owned()
                + &script
                + r#"
G = {C = {WHITE = {1, 1, 1, 1}}}
local element = {config = {colour = {0.2, 0.3, 0.4, 0.5}}}
local colour = SVMM_UI_JIT_FUNCTIONS.normalize_colour(element, 0.25)
return colour[1], colour[2], colour[3], colour[4], G._svmm_invalid_ui_colours
"#,
        )
        .eval()
        .expect("invalid UI colour should use the element colour");
    assert_eq!(fallback, (0.2, 0.3, 0.4, 0.5, 1));
}

#[test]
fn miyoo_ui_draw_recovers_from_invalid_colours_once() {
    let mut script =
        b"function UIElement:draw_self()\nend\nfunction UIElement:draw_pixellated_rect()\nend"
            .to_vec();
    patch_miyoo_script("engine/ui.lua", &mut script);
    let script = String::from_utf8(script).unwrap();
    assert!(script.contains("G._svmm_invalid_ui_colours"));
    assert!(script.contains("G._svmm_reported_invalid_ui_colour"));
    assert!(script.contains("type(configured) == 'table'"));
}

#[test]
fn miyoo_ui_tree_preserves_draw_order() {
    let mut script = b"UIElement = {}\nfunction UIElement:draw_self()\nend\nfunction UIElement:draw_pixellated_rect()\nend"
            .to_vec();
    patch_miyoo_script("engine/ui.lua", &mut script);
    let mut script = String::from_utf8(script).unwrap();
    script.push_str(
        r#"
local order = {}
UIElement.draw_self = function(self) order[#order + 1] = self.name end
local function element(name, config, children, visible)
    local value = {
        name = name,
        config = config or {},
        children = children or {},
        states = {visible = visible ~= false}
    }
    return setmetatable(value, {__index = UIElement})
end
local child = element('child')
local after = element('after', {draw_after = true}, {child})
local normal = element('normal')
local hidden = element('hidden', {}, {element('hidden-child')}, false)
local layered = element('layered', {draw_layer = 1})
local custom = {
    name = 'custom', config = {}, states = {visible = true},
    draw = function(self) order[#order + 1] = self.name end
}
local root = element('root', {}, {after, normal, hidden, layered, custom})
root:draw_children()
return table.concat(order, ',')
"#,
    );
    let order: String = mlua::Lua::new()
        .load(&script)
        .eval()
        .expect("specialized UI tree should execute");
    assert_eq!(order, "child,after,normal,hidden,custom,custom");
}

#[test]
fn miyoo_ui_draws_named_controller_prompts_but_not_separate_popups() {
    let mut script = b"UIElement = {}\nfunction UIElement:draw_self()\nend\nfunction UIElement:draw_pixellated_rect()\nend".to_vec();
    patch_miyoo_script("engine/ui.lua", &mut script);
    let lua = mlua::Lua::new();
    lua.load(script).exec().unwrap();
    lua.load(
        r#"
            local seen = {}
            UIElement.draw_self = function(self) seen[self.name] = true end
            local function element(name)
                return setmetatable({name = name, config = {}, children = {},
                    states = {visible = true}}, {__index = UIElement})
            end
            local root = element('root')
            root.children = {element('text'), button_pip = element('prompt'),
                h_popup = element('popup'), alert = element('alert')}
            root:draw_children()
            assert(seen.text, 'text was not drawn')
            assert(seen.prompt, 'named controller prompt was not drawn')
            assert(not seen.popup and not seen.alert, 'separate popup was drawn twice')
        "#,
    )
    .exec()
    .unwrap();
}

#[test]
fn miyoo_static_ui_commands_reuse_and_invalidate() {
    let mut script = b"UIElement = {}\nfunction UIElement:draw_self()\nend\nfunction UIElement:draw_pixellated_rect()\n    polygon_calls = polygon_calls + 1\n    return polygon\nend"
            .to_vec();
    patch_miyoo_script("engine/ui.lua", &mut script);
    let mut script = String::from_utf8(script).unwrap();
    script.push_str(
        r#"
local misses, allocations = 0, 0
polygon_calls = 0
local first
local second
love = {graphics = {}}
love.graphics._drawUIBatch = function(commands, count)
    assert(count == 1)
    local command = commands[1]
    if not command._native_ui_draw then
        allocations = allocations + 1
        command._native_ui_draw = {}
    end
    if command._native_ui_dirty then
        misses = misses + 1
        command._native_ui_dirty = false
    end
    if not first then first = command else second = command end
end
G = {
    UIT = {T = 1, B = 2, C = 3, R = 4, ROOT = 5},
    TILESCALE = 1,
    TILESIZE = 1
}
polygon = {}
local element = {
    UIT = G.UIT.C,
    UIBox = {},
    config = {r = 0.2, colour = {0.2, 0.3, 0.4, 1}},
    states = {visible = true, collide = {can = false}, focus = {is = false}},
    VT = {x = 1, y = 2, w = 3, h = 4, r = 0, scale = 1},
    layered_parallax = {x = 0, y = 0},
    shadow_parrallax = {x = 0, y = 0},
    STATIONARY = true
}
setmetatable(element, {__index = UIElement})
element:draw_self()
SVMM_flush_ui_draws()
element:draw_self()
SVMM_flush_ui_draws()
local reused = first == second
element.VT.x = 2
element:draw_self()
SVMM_flush_ui_draws()
element.shadow_parrallax.x = 0.5
element:draw_self()
SVMM_flush_ui_draws()
return reused, misses, polygon_calls, allocations
"#,
    );
    let result: (bool, i64, i64, i64) = mlua::Lua::new()
        .load(&script)
        .eval()
        .expect("static UI command cache should execute");
    assert_eq!(result, (true, 3, 2, 1));
}

#[test]
fn miyoo_draw_hash_tracks_overlay_without_debug_draw_calls() {
    let mut misc = b"function add_to_drawhash(obj)\n  if obj then \n    G.DRAW_HASH[#G.DRAW_HASH+1] = obj\n  end\nend"
            .to_vec();
    patch_miyoo_script("functions/misc_functions.lua", &mut misc);
    let misc = String::from_utf8(misc).unwrap();
    assert!(misc.contains("obj.under_overlay = G.under_overlay"));

    let mut node =
        b"function Node:draw()\n    self:draw_boundingrect()\n    add_to_drawhash(self)\nend"
            .to_vec();
    patch_miyoo_script("engine/node.lua", &mut node);
    let node = String::from_utf8(node).unwrap();
    assert!(!node.contains("self:draw_boundingrect()"));
}

#[test]
fn miyoo_prep_draw_batches_transform_setup() {
    let mut script = b"function prep_draw(moveable, scale, rotate, offset)\n    love.graphics.push()\n    love.graphics.scale(G.TILESCALE*G.TILESIZE)\nend\n\nfunction get_chosen_triangle_from_rect()\nend"
            .to_vec();
    patch_miyoo_script("functions/misc_functions.lua", &mut script);
    let script = String::from_utf8(script).unwrap();
    assert!(script.contains("love.graphics._pushTransform("));
    assert!(!script.contains("love.graphics.push()"));
    mlua::Lua::new()
        .load(&script)
        .into_function()
        .expect("batched transform setup should parse");
}

#[test]
fn miyoo_sprite_draw_uses_single_software_shader_update() {
    let mut script = b"function Sprite:draw_shader(_shader, _shadow_height, _send, _no_tilt, other_obj, ms, mr, mx, my, custom_shader, tilt_shadow)\n    if G.SETTINGS.reduced_motion then _no_tilt = true end\nend".to_vec();
    patch_miyoo_script("engine/sprite.lua", &mut script);
    let script = String::from_utf8(script).unwrap();
    assert!(script.contains("love.graphics._setSoftwareShader("));
    assert!(script.contains("if not custom_shader and _shader ~= 'vortex' then"));
    let lua = mlua::Lua::new();
    lua.load("Sprite = {}\n".to_owned() + &script)
        .exec()
        .unwrap();
    lua.load(
        r#"
            local sent, drawn, reset
            love = {graphics = {
                _setSoftwareShader = function(...) sent = {...} end,
                setShader = function() reset = true end
            }}
            G = {SETTINGS = {}, SHADERS = {holo = {}}, C = {CLEAR = {0, 0, 0, 0}}}
            local values = {1.25, 42}
            local major = {ID = 12, dissolve = 0.3}
            local sprite = setmetatable({role = {draw_major = major},
                draw_self = function() drawn = true end}, {__index = Sprite})
            sprite:draw_shader('holo', nil, values)
            assert(sent[1] == G.SHADERS.holo and sent[7] == values)
            assert(sent[2] == 0.3 and sent[3] == false)
            assert(sent[4] == 123.33412*(major.ID/1.14212)%3000)
            assert(drawn and reset)
        "#,
    )
    .exec()
    .unwrap();
}

#[test]
fn miyoo_cards_skip_the_disabled_shadow_pass() {
    let mut card_area =
            b"self.ARGS.draw_layers = self.ARGS.draw_layers or self.config.draw_layers or {'shadow', 'card'}"
                .to_vec();
    patch_miyoo_script("cardarea.lua", &mut card_area);
    let card_area = String::from_utf8(card_area).unwrap();
    assert!(card_area.contains("self.config.draw_layers or {'card'}"));
    assert!(!card_area.contains("{'shadow', 'card'}"));

    let mut card = b"    self.children.shadow = Moveable(0, 0, 0, 0)\nif (layer == 'shadow' or layer == 'both') then".to_vec();
    patch_miyoo_script("card.lua", &mut card);
    let card = String::from_utf8(card).unwrap();
    assert!(!card.contains("children.shadow"));
    assert!(card.contains("layer == 'card' and G.SETTINGS.GRAPHICS.shadows ~= 'On'"));
}

#[test]
fn miyoo_plain_cards_bypass_special_effect_branches() {
    let mut card = b"function Card:draw(layer)\n    layer = layer or 'both'\n\n    self.hover_tilt = 1\n    \n    if not self.states.visible then return end\n    self.children.center:draw_shader('dissolve')\nend"
            .to_vec();
    patch_miyoo_script("card.lua", &mut card);
    let card = String::from_utf8(card).unwrap();
    assert!(card.contains("local function SVMM_draw_plain_card(card)"));
    assert!(card.contains("self.config.center == G.P_CENTERS.c_base"));
    assert!(card.contains("SVMM_draw_plain_card(self)"));
    assert!(card.contains("children.center:draw_shader('dissolve')"));
    mlua::Lua::new()
        .load("Card = {}\n".to_owned() + &card)
        .into_function()
        .expect("plain card fast path should parse");
}

#[test]
fn miyoo_skips_idle_base_card_updates() {
    let mut card = b"function Card:update(dt)\n    if self.flipping == 'f2b' then\n        self.updated = true\n    end\nend"
            .to_vec();
    patch_miyoo_script("card.lua", &mut card);
    let card = String::from_utf8(card).unwrap();
    assert!(card.contains("self.config.center == G.P_CENTERS.c_base"));
    assert!(card.contains("not self.ability.perma_debuff then return"));
    mlua::Lua::new()
        .load("Card = {}\n".to_owned() + &card)
        .into_function()
        .expect("base card update fast path should parse");
}

#[test]
fn miyoo_moveable_update_handles_removal_during_iteration() {
    let mut script = b"local calls = {}\nNode = {update = function() calls[#calls + 1] = 'idle' end}\nlocal game = {SPEEDFACTOR = 1, MOVEABLES = {}, SVMM_UPDATEABLES = {}}\nlocal self = game\nlocal first = {FRAME = {MOVE = 1}, states = {collide = {is = true}}}\nlocal second = {FRAME = {MOVE = 1}, states = {collide = {is = true}}}\nlocal idle = {FRAME = {MOVE = 1}, states = {collide = {is = false}}, update = Node.update}\nfunction first:update() calls[#calls + 1] = 'first'; table.remove(game.MOVEABLES, 1); table.remove(game.SVMM_UPDATEABLES, 1) end\nfunction second:update() calls[#calls + 1] = 'second' end\nself.MOVEABLES = {first, second, idle}\nself.SVMM_UPDATEABLES = {first, second}\nself.CONTROLLER = {collision_list = {first}, dragging = {target = second}}\nG = {FRAMES = {MOVE = 1}}\nlocal move_dt, dt = 0, 0\nfunction timer_checkpoint() end\n        for k, v in pairs(self.MOVEABLES) do\n            if v.FRAME.MOVE < G.FRAMES.MOVE then v:move(move_dt) end\n        end\n                    timer_checkpoint('move', 'update')\n        \n        for k, v in pairs(self.MOVEABLES) do\n            v:update(dt*self.SPEEDFACTOR)\n            v.states.collide.is = false\n        end\nreturn table.concat(calls, ',') .. ':' .. tostring(first.states.collide.is) .. ':' .. tostring(second.states.collide.is)"
            .to_vec();
    patch_miyoo_script("game.lua", &mut script);
    let lua = mlua::Lua::new();
    let calls: String = lua
        .load(&script)
        .eval()
        .expect("patched moveable loop should execute");
    assert_eq!(calls, "first,second:false:false");
}

#[test]
fn miyoo_moveable_loop_resolves_parents_before_children() {
    let mut script = concat!(r#"
local calls = {}
Node = {update = function() end}
Moveable = {_svmm_role_lists = true, move = function(self) calls[#calls + 1] = self.name end}
local function moveable(x, visible_x)
    local item = {
        FRAME = {MOVE = 0},
        role = {role_type = 'Major', offset = {x = 0, y = 0}},
        velocity = {x = 0, y = 0, r = 0, scale = 0},
        states = {drag = {is = false}, hover = {is = false}, collide = {is = true}},
        alignment = {
            lr_clamp = false,
            offset = {x = 0, y = 0},
            prev_offset = {x = 0, y = 0},
            type = 'a',
            prev_type = 'a'
        },
        config = {refresh_movement = false},
        pinch = {x = false, y = false},
        T = {x = x, y = 0, w = 1, h = 1, r = 0, scale = 1},
        VT = {x = visible_x, y = 0, w = 1, h = 1, r = 0, scale = 1},
        STATIONARY = true
    }
    item.move = Moveable.move
    function item:update() end
    return item
end
local settled = moveable(0, 0)
settled.name = 'settled'
local moving = moveable(1, 0)
moving.name = 'moving'
local parent = moveable(0, 0)
parent.name = 'parent'
parent.FRAME.MOVE = 1
local stable_minor = moveable(0, 0)
stable_minor.name = 'stable_minor'
stable_minor.role = {role_type = 'Minor', major = parent, offset = {x = 0, y = 0}}
local stable_refresh = moveable(0, 0)
stable_refresh.name = 'stable_refresh'
stable_refresh.role = {role_type = 'Minor', major = parent, offset = {x = 2, y = 3}}
stable_refresh.config.refresh_movement = true
stable_refresh.layered_parallax = {x = 0.25, y = -0.5}
stable_refresh._svmm_refresh_major = parent
stable_refresh._svmm_refresh_offset_x = 2
stable_refresh._svmm_refresh_offset_y = 3
stable_refresh._svmm_refresh_layered_x = 0.25
stable_refresh._svmm_refresh_layered_y = -0.5
local changed_refresh = moveable(0, 0)
changed_refresh.name = 'changed_refresh'
changed_refresh.role = {role_type = 'Minor', major = parent, offset = {x = 4, y = 5}}
changed_refresh.config.refresh_movement = true
changed_refresh.layered_parallax = {x = 0, y = 0}
changed_refresh._svmm_refresh_major = parent
changed_refresh._svmm_refresh_offset_x = 3
changed_refresh._svmm_refresh_offset_y = 5
changed_refresh._svmm_refresh_layered_x = 0
changed_refresh._svmm_refresh_layered_y = 0
local stale_parent = moveable(0, 0)
stale_parent.name = 'stale_parent'
local stale_minor = moveable(0, 0)
stale_minor.name = 'stale_minor'
stale_minor.role = {role_type = 'Minor', major = stale_parent, offset = {x = 0, y = 0}}
local stable_glued = moveable(0, 0)
stable_glued.name = 'stable_glued'
stable_glued.STATIONARY = false
stable_glued.role = {role_type = 'Glued', major = parent, offset = {x = 0, y = 0}}
stable_glued._svmm_glued_major = parent
stable_glued.T = parent.T
stable_glued.VT = {x = 0, y = 0, w = 1, h = 1, r = 0, scale = 1}
stable_glued.pinch = parent.pinch
stable_glued.shadow_parrallax = parent.shadow_parrallax
local stale_glued = moveable(0, 0)
stale_glued.name = 'stale_glued'
stale_glued.STATIONARY = false
stale_glued.role = {role_type = 'Glued', major = stale_parent, offset = {x = 0, y = 0}}
stale_glued.T = stale_parent.T
stale_glued.VT = {x = 0, y = 0, w = 1, h = 1, r = 0, scale = 1}
stale_glued.pinch = stale_parent.pinch
stale_glued.shadow_parrallax = stale_parent.shadow_parrallax
local late_parent = moveable(0, 0)
late_parent.name = 'late_parent'
local deferred_glued = moveable(0, 0)
deferred_glued.name = 'deferred_glued'
deferred_glued.STATIONARY = false
deferred_glued.role = {role_type = 'Glued', major = late_parent, offset = {x = 0, y = 0}}
deferred_glued._svmm_glued_major = late_parent
deferred_glued.T = late_parent.T
deferred_glued.pinch = late_parent.pinch
deferred_glued.shadow_parrallax = late_parent.shadow_parrallax
local glued_chain = moveable(0, 0)
glued_chain.name = 'glued_chain'
glued_chain.STATIONARY = false
glued_chain.role = {role_type = 'Glued', major = deferred_glued, offset = {x = 0, y = 0}}
glued_chain._svmm_glued_major = deferred_glued
glued_chain.T = deferred_glued.T
glued_chain.pinch = deferred_glued.pinch
glued_chain.shadow_parrallax = deferred_glued.shadow_parrallax
local self = {SPEEDFACTOR = 1, MOVEABLES = {
    settled, moving, stable_minor, stable_refresh, changed_refresh, stale_parent, stale_minor,
    stable_glued, stale_glued, glued_chain, deferred_glued, late_parent
}, SVMM_UPDATEABLES = {
    settled, moving, stable_minor, stable_refresh, changed_refresh, stale_minor,
    stable_glued, stale_glued
}}
G = {FRAMES = {MOVE = 1}}
_SVMM_PHASE_PROFILE = true
local move_dt, dt = 0, 0
function timer_checkpoint() end
        for k, v in pairs(self.MOVEABLES) do
            if v.FRAME.MOVE < G.FRAMES.MOVE then v:move(move_dt) end
        end
                    timer_checkpoint('move', 'update')
"#, "        ", r#"
        for k, v in pairs(self.MOVEABLES) do
            v:update(dt*self.SPEEDFACTOR)
            v.states.collide.is = false
        end
return table.concat(calls, ','), #calls, self._svmm_active_count, settled.FRAME.MOVE, moving.FRAME.MOVE,
    stable_minor.FRAME.MOVE, stable_refresh.FRAME.MOVE, changed_refresh.FRAME.MOVE,
    stale_minor.FRAME.MOVE,
    stable_glued.FRAME.MOVE, stale_glued.FRAME.MOVE,
    deferred_glued.FRAME.MOVE, late_parent.FRAME.MOVE,
    glued_chain.FRAME.MOVE
"#)
        .as_bytes()
        .to_vec();
    patch_miyoo_script("game.lua", &mut script);
    let lua = mlua::Lua::new();
    let (
        call_names,
        calls,
        active_count,
        settled_frame,
        moving_frame,
        stable_minor_frame,
        stable_refresh_frame,
        changed_refresh_frame,
        stale_minor_frame,
        stable_glued_frame,
        stale_glued_frame,
        deferred_glued_frame,
        late_parent_frame,
        glued_chain_frame,
    ): (
        String,
        u32,
        u32,
        u32,
        u32,
        u32,
        u32,
        u32,
        u32,
        u32,
        u32,
        u32,
        u32,
        u32,
    ) = lua
        .load(&script)
        .eval()
        .expect("patched moveable loop should execute");
    assert_eq!(call_names, "moving,changed_refresh,stale_glued");
    assert_eq!(calls, 3);
    assert_eq!(active_count, 4);
    assert_eq!(settled_frame, 1);
    assert_eq!(moving_frame, 0);
    assert_eq!(stable_minor_frame, 1);
    assert_eq!(stable_refresh_frame, 1);
    assert_eq!(changed_refresh_frame, 0);
    assert_eq!(stale_minor_frame, 1);
    assert_eq!(stable_glued_frame, 1);
    assert_eq!(stale_glued_frame, 0);
    assert_eq!(deferred_glued_frame, 1);
    assert_eq!(late_parent_frame, 1);
    assert_eq!(glued_chain_frame, 1);
}

#[test]
fn classified_minor_chains_skip_when_children_precede_parents() {
    let script = format!(
        r#"
Moveable = {{}}
G = {{
    FRAMES = {{MOVE = 2}},
    SVMM_MAJORS = {{}},
    SVMM_MINORS = {{}},
    SVMM_GLUED = {{}},
    _svmm_role_lists_ready = true
}}
function timer_checkpoint() end
_SVMM_PHASE_PROFILE = true
{MIYOO_CLASSIFIED_MOVEABLE_SCAN}
local calls = 0
function Moveable:move()
    calls = calls + 1
    self.FRAME.MOVE = G.FRAMES.MOVE
end
local function moveable(role_type, major)
    local item = {{
        FRAME = {{MOVE = 1}},
        STATIONARY = true,
        NEW_ALIGNMENT = false,
        role = {{
            role_type = role_type,
            major = major,
            offset = {{x = 0, y = 0}},
            xy_bond = 'Strong', wh_bond = 'Strong',
            r_bond = 'Strong', scale_bond = 'Strong'
        }},
        alignment = {{
            prev_type = 'cm', type = 'cm',
            prev_offset = {{x = 0, y = 0}}, offset = {{x = 0, y = 0}}
        }},
        config = {{refresh_movement = false}},
        layered_parallax = {{x = 0, y = 0}},
        T = {{x = 0, y = 0, w = 1, h = 1, r = 0, scale = 1}},
        VT = {{x = 0, y = 0, w = 1, h = 1, r = 0, scale = 1}},
        velocity = {{x = 0, y = 0, r = 0, scale = 0}}
    }}
    item.move = Moveable.move
    return item
end
local root = moveable('Major')
root.FRAME.MOVE = G.FRAMES.MOVE
local parent = moveable('Minor', root)
local child = moveable('Minor', parent)
G.SVMM_MAJORS = {{root}}
G.SVMM_MINORS = {{child, parent}}
local game = {{
    MOVEABLES = {{root, parent, child}},
    SVMM_UPDATEABLES = {{}},
    SPEEDFACTOR = 1
}}
SVMM_update_moveables(game, 0.016, 0.016)
return calls, parent.FRAME.MOVE, child.FRAME.MOVE, game._svmm_active_count
"#
    );
    let result: (u32, u32, u32, u32) = mlua::Lua::new()
        .load(&script)
        .eval()
        .expect("classified minor chain should execute");
    assert_eq!(result, (0, 2, 2, 0));
}

#[test]
fn background_cache_refreshes_for_palette_or_size_changes() {
    let colours = [[0.0; 4]; 3];
    let mut cache = BackgroundCache::new();
    cache.buffer.resize(320, 240);
    cache.colours = colours;
    cache.valid = true;

    assert!(!cache.needs_refresh(320, 240, &colours));
    assert!(cache.needs_refresh(640, 480, &colours));
    assert!(cache.needs_refresh(320, 240, &[[1.0; 4]; 3]));
}
