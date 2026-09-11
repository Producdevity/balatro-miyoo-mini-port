const MIYOO_UI_DRAW_SELF: &str = include_str!("ui_draw.lua");

const MIYOO_SPRITE_SHADER_FAST_PATH: &str = r#"
    if not custom_shader and _shader ~= 'vortex' then
        local draw_major = self.role.draw_major or self
        if _shadow_height then
            self.VT.y = self.VT.y - draw_major.shadow_parrallax.y*_shadow_height
            self.VT.x = self.VT.x - draw_major.shadow_parrallax.x*_shadow_height
            self.VT.scale = self.VT.scale*(1-0.2*_shadow_height)
        end

        local dissolve_colours = draw_major.dissolve_colours
        love.graphics._setSoftwareShader(
            G.SHADERS[_shader or 'dissolve'],
            math.abs(draw_major.dissolve or 0),
            not not _shadow_height,
            123.33412*(draw_major.ID/1.14212 or 12.5123152)%3000,
            dissolve_colours and dissolve_colours[1] or G.C.CLEAR,
            dissolve_colours and dissolve_colours[2] or G.C.CLEAR,
            _send
        )

        if other_obj then
            self:draw_from(other_obj, ms, mr, mx, my)
        else
            self:draw_self()
        end
        love.graphics.setShader()

        if _shadow_height then
            self.VT.y = self.VT.y + draw_major.shadow_parrallax.y*_shadow_height
            self.VT.x = self.VT.x + draw_major.shadow_parrallax.x*_shadow_height
            self.VT.scale = self.VT.scale/(1-0.2*_shadow_height)
        end
        return
    end
"#;

const MIYOO_CLASSIFIED_MOVEABLE_SCAN: &str = include_str!("classified_moveables.lua");

pub const MIYOO_SMALL_SCREEN_PATCH: &str = concat!(
    include_str!("small_screen.lua"),
    "\n",
    include_str!("popups.lua")
);
pub const MIYOO_PAYOUT_PATCH: &str = include_str!("payout.lua");

pub(crate) fn patch_controller_input(data: &mut Vec<u8>) {
    const PHASE: &str = "    self:set_HID_flags(self:update_axis(dt))";
    let Ok(source) = std::str::from_utf8(data) else {
        return;
    };
    if source.contains(PHASE) && !source.contains("Controller.miyoo_input_phase = true") {
        *data = format!(
            "{}\nController.miyoo_input_phase = true\n",
            source.replacen(
                PHASE,
                &format!("{PHASE}\n    if self.miyoo_dispatch then self:miyoo_dispatch(dt) end"),
                1
            )
        )
        .into_bytes();
    }
}

const MIYOO_MOVEABLE_MOVE: &str = r#"function Moveable:move(dt, classified_active)
    local game = G
    local frame_move = game.FRAMES.MOVE
    local frame = self.FRAME
    if frame.MOVE >= frame_move then return end
    frame.OLD_MAJOR = frame.MAJOR
    frame.MAJOR = nil
    frame.MOVE = frame_move
    if not self.created_on_pause and game.SETTINGS.paused then return end

    local alignment = self.alignment
    local previous, offset = alignment.prev_offset, alignment.offset
    if alignment.prev_type ~= alignment.type or
        previous.x ~= offset.x or previous.y ~= offset.y then
        Moveable.align_to_major(self)
    end
    self.CALCING = nil

    local role = self.role
    local role_type = role.role_type
    if not classified_active and role_type == 'Major' and self.STATIONARY then
        local transform, visible = self.T, self.VT
        local velocity, pinch, states = self.velocity, self.pinch, self.states
        if not self.NEW_ALIGNMENT and not self.config.refresh_movement and
            not self.juice and not alignment.lr_clamp and
            not pinch.x and not pinch.y and
            not (self.zoom and (states.drag.is or states.hover.is)) and
            transform.x == visible.x and transform.y == visible.y and
            transform.w == visible.w and transform.h == visible.h and
            transform.r == visible.r and transform.scale == visible.scale and
            velocity.x == 0 and velocity.y == 0 and
            velocity.r == 0 and velocity.scale == 0 then
                self.NEW_ALIGNMENT = false
                return
        end
    end

    if role_type == 'Glued' then
        local major = role.major
        if major then Moveable.glue_to_major(self, major) end
    elseif role_type == 'Minor' then
        local major = role.major
        if major then
            if major.FRAME.MOVE < frame_move then major:move(dt) end
            self.STATIONARY = major.STATIONARY
            if not self.STATIONARY or self.NEW_ALIGNMENT or
                self.config.refresh_movement or self.juice or
                role.xy_bond == 'Weak' or role.r_bond == 'Weak' then
                    self.CALCING = true
                    Moveable.move_with_major(self, dt)
            end
        end
    elseif role_type == 'Major' then
        self.STATIONARY = true
        local transform, visible = self.T, self.VT
        local velocity, pinch, states = self.velocity, self.pinch, self.states
        local juice = self.juice
        if juice then Moveable.move_juice(self, dt); juice = self.juice end
        if transform.x ~= visible.x or transform.y ~= visible.y or
            math.abs(velocity.x) > 0.01 or math.abs(velocity.y) > 0.01 then
            Moveable.move_xy(self, dt)
        end
        if transform.r ~= visible.r or math.abs(velocity.r) > 0.001 or
            velocity.x ~= 0 or (juice and juice.r ~= 0) then
            Moveable.move_r(self, dt, velocity)
        end
        if transform.scale ~= visible.scale or math.abs(velocity.scale) > 0.001 or
            juice or (self.zoom and (states.drag.is or states.hover.is)) then
            Moveable.move_scale(self, dt)
        end
        if (transform.w ~= visible.w and not pinch.x) or
            (transform.h ~= visible.h and not pinch.y) or
            (visible.w > 0 and pinch.x) or (visible.h > 0 and pinch.y) then
            Moveable.move_wh(self, dt)
        end
        local room = game.ROOM
        if room and (self._svmm_parallax_x ~= transform.x or
            self._svmm_parallax_w ~= transform.w or
            self._svmm_parallax_room_w ~= room.T.w) then
            self.shadow_parrallax.x =
                (transform.x + transform.w/2 - room.T.w/2)/(room.T.w/2)*1.5
            self._svmm_parallax_x = transform.x
            self._svmm_parallax_w = transform.w
            self._svmm_parallax_room_w = room.T.w
        end
    end
    if alignment and alignment.lr_clamp then Moveable.lr_clamp(self) end
    self.NEW_ALIGNMENT = false
end

"#;

const MIYOO_MOVE_WITH_MAJOR: &str = r#"function Moveable:move_with_major(dt)
    local role = self.role
    if role.role_type ~= 'Minor' then return end

    local major_tab = Moveable.get_major(role.major)
    local major = major_tab.major
    local offset_x = role.offset.x + major_tab.offset.x
    local offset_y = role.offset.y + major_tab.offset.y
    local weak_r = role.r_bond == 'Weak'
    local rotated_x, rotated_y
    if weak_r then
        rotated_x, rotated_y = offset_x, offset_y
    else
        local rotation = major.VT.r
        if rotation < 0.0001 and rotation > -0.0001 then
            rotated_x, rotated_y = offset_x, offset_y
        else
            local transform = self.T
            local half_w = -transform.w/2 + major.T.w/2
            local half_h = -transform.h/2 + major.T.h/2
            local adjusted_x = offset_x - half_w
            local adjusted_y = offset_y - half_h
            local cosine, sine = math.cos(rotation), math.sin(rotation)
            rotated_x = adjusted_x*cosine - adjusted_y*sine + half_w
            rotated_y = adjusted_x*sine + adjusted_y*cosine + half_h
        end
    end

    Moveable.move_juice(self, dt)
    local transform, visible = self.T, self.VT
    transform.x = major.T.x + rotated_x
    transform.y = major.T.y + rotated_y

    if role.xy_bond == 'Weak' then
        Moveable.move_xy(self, dt)
    else
        visible.x = major.VT.x + rotated_x
        visible.y = major.VT.y + rotated_y
    end

    local juice = self.juice
    if weak_r then
        Moveable.move_r(self, dt, self.velocity)
    else
        visible.r = transform.r + major.VT.r + (juice and juice.r or 0)
    end

    if role.scale_bond == 'Weak' then
        Moveable.move_scale(self, dt)
    else
        visible.scale = transform.scale*(major.VT.scale/major.T.scale) +
            (juice and juice.scale or 0)
    end

    if role.wh_bond == 'Weak' then
        Moveable.move_wh(self, dt)
    else
        visible.x = visible.x + (0.5*(1 - major.VT.w/major.T.w)*transform.w)
        visible.w = transform.w*(major.VT.w/major.T.w)
        visible.h = transform.h*(major.VT.h/major.T.h)
    end

    local room = G.ROOM
    if room and (self._svmm_parallax_x ~= transform.x or
        self._svmm_parallax_w ~= transform.w or
        self._svmm_parallax_room_w ~= room.T.w) then
        self.shadow_parrallax.x =
            (transform.x + transform.w/2 - room.T.w/2)/(room.T.w/2)*1.5
        self._svmm_parallax_x = transform.x
        self._svmm_parallax_w = transform.w
        self._svmm_parallax_room_w = room.T.w
    end
end

"#;

const MIYOO_PREP_DRAW: &str = r#"function prep_draw(moveable, scale, rotate, offset)
    local visible = moveable.VT
    local layered = moveable.layered_parallax
    local parent = moveable.parent
    local parent_layered = parent and parent.layered_parallax
    local offset_x = offset and offset.x or 0
    local offset_y = offset and offset.y or 0
    local parallax_x = (layered and layered.x) or (parent_layered and parent_layered.x) or 0
    local parallax_y = (layered and layered.y) or (parent_layered and parent_layered.y) or 0
    local object_scale = visible.scale*scale
    love.graphics._pushTransform(
        G.TILESCALE*G.TILESIZE,
        visible.x+visible.w/2+offset_x+parallax_x,
        visible.y+visible.h/2+offset_y+parallax_y,
        visible.r+(rotate or 0),
        -scale*visible.w*visible.scale/2,
        -scale*visible.h*visible.scale/2,
        object_scale)
end
"#;

const MIYOO_CARD_DRAW_FAST_PATH: &str = r#"local function SVMM_draw_plain_card(card)
    local states = card.states
    local tilt = card.tilt_var
    if not tilt then
        tilt = {mx = 0, my = 0, dx = 0, dy = 0, amt = 0}
        card.tilt_var = tilt
    end
    local tilt_factor = 0.3
    if states.focus.is then
        tilt.mx = G.CONTROLLER.cursor_position.x + tilt.dx*card.T.w*G.TILESCALE*G.TILESIZE
        tilt.my = G.CONTROLLER.cursor_position.y + tilt.dy*card.T.h*G.TILESCALE*G.TILESIZE
        tilt.amt = math.abs(card.hover_offset.y + card.hover_offset.x - 1 + tilt.dx + tilt.dy - 1)*tilt_factor
    elseif states.hover.is then
        tilt.mx, tilt.my = G.CONTROLLER.cursor_position.x, G.CONTROLLER.cursor_position.y
        tilt.amt = math.abs(card.hover_offset.y + card.hover_offset.x - 1)*tilt_factor
    elseif card.ambient_tilt then
        local tilt_angle = G.TIMERS.REAL*(1.56 + (card.ID/1.14212)%1) + card.ID/1.35122
        tilt.mx = ((0.5 + 0.5*card.ambient_tilt*math.cos(tilt_angle))*card.VT.w+card.VT.x+G.ROOM.T.x)*G.TILESIZE*G.TILESCALE
        tilt.my = ((0.5 + 0.5*card.ambient_tilt*math.sin(tilt_angle))*card.VT.h+card.VT.y+G.ROOM.T.y)*G.TILESIZE*G.TILESCALE
        tilt.amt = card.ambient_tilt*(0.5+math.cos(tilt_angle))*tilt_factor
    end

    local children = card.children
    if card.area ~= G.hand and children.focused_ui then children.focused_ui:draw() end
    children.center:draw_shader('dissolve')
    if children.front then children.front:draw_shader('dissolve') end
    for key, child in pairs(children) do
        if key ~= 'focused_ui' and key ~= 'front' and key ~= 'back' and
            key ~= 'soul_parts' and key ~= 'center' and key ~= 'floating_sprite' and
            key ~= 'shadow' and key ~= 'use_button' and key ~= 'buy_button' and
            key ~= 'buy_and_use_button' and key ~= 'debuff' and key ~= 'price' and
            key ~= 'particles' and key ~= 'h_popup' then
            child:draw()
        end
    end
    if card.area == G.hand and children.focused_ui then children.focused_ui:draw() end
    add_to_drawhash(card)
end

"#;

const MIYOO_CARD_DRAW_ENTRY: &str = r#"function Card:draw(layer)
    layer = layer or 'both'

    self.hover_tilt = 1

    if not self.states.visible then return end

    local children = self.children
    if layer == 'card' and self.sprite_facing == 'front' and not self.vortex and
        self.ability.set == 'Default' and self.config.center == G.P_CENTERS.c_base and
        not self.edition and not self.seal and not self.debuff and not self.greyed and
        not self.sticker and not self.sticker_run and
        not children.particles and not children.price and not children.buy_button and
        not children.buy_and_use_button and not children.use_button then
        G.shared_shadow = children.center
        SVMM_draw_plain_card(self)
        return
    end
"#;

pub(crate) fn patch_miyoo_script(file_path: &str, data: &mut Vec<u8>) {
    let normalized = file_path.replace('\\', "/");
    let replacements: &[(&str, &str)] = match normalized.as_str() {
        "globals.lua" => &[
            (
                "self.F_MUTE = false",
                "self.F_MUTE = os.getenv('BALATRO_AUDIO') ~= '1'",
            ),
            ("self.F_SOUND_THREAD = true", "self.F_SOUND_THREAD = false"),
            ("self.F_VIDEO_SETTINGS = true", "self.F_VIDEO_SETTINGS = false"),
            ("self.F_VERBOSE = true", "self.F_VERBOSE = false"),
            ("texture_scaling = 2", "texture_scaling = 1"),
            ("shadows = 'On'", "shadows = 'Off'"),
            (
                "self.TILE_H = 11.5",
                "self.TILE_H = 11.5\n    self.SVMM_HANDHELD_LAYOUT = os.getenv('BALATRO_HANDHELD_LAYOUT') ~= '0'",
            ),
            (
                "self.MOVEABLES = {}",
                "self.MOVEABLES = {}\n    self.SVMM_UPDATEABLES = {}\n    self.SVMM_MAJORS = {}\n    self.SVMM_MINORS = {}\n    self.SVMM_GLUED = {}",
            ),
        ],
        "game.lua" => &[
            (
                "config = {align='tm', offset = {x=0,y=-0.8},major = self.hand, bond = 'Weak'}",
                "config = {instance_type='POPUP', align='tm', offset = {x=0,y=-0.8},major = self.hand, bond = 'Weak'}",
            ),
            (
                "function Game:update(dt)",
                "SVMM_UPDATE_JIT_FUNCTIONS = {score_intensity = modulate_sound}\n\nfunction Game:update(dt)",
            ),
            (
                "self.SETTINGS.GRAPHICS.texture_scaling = self.SETTINGS.GRAPHICS.texture_scaling or 2",
                "self.SETTINGS.GRAPHICS.texture_scaling = 1\n    self.SETTINGS.GRAPHICS.shadows = 'Off'\n    self.SETTINGS.GRAPHICS.crt = 0\n    self.SETTINGS.GRAPHICS.bloom = 0",
            ),
            (
                "config = {align=('cli'), offset = {x=-0.7,y=0},major = G.ROOM_ATTACH}",
                "config = {align=('cli'), offset = {x=G.SVMM_HANDHELD_LAYOUT and 0 or -0.7,y=0},major = G.ROOM_ATTACH}",
            ),
            ("render_scale = self.TILESIZE*10", "render_scale = self.TILESIZE*2.5"),
            (
                "FONTSCALE = 0.1, squish",
                "FONTSCALE = 0.4, LAYOUT_FONTSCALE = 0.1, squish",
            ),
            ("render_scale = self.TILESIZE*7", "render_scale = self.TILESIZE*2"),
            (
                "FONTSCALE = 0.12, squish",
                "FONTSCALE = 0.42, LAYOUT_FONTSCALE = 0.12, squish",
            ),
            (
                "love.graphics.setCanvas{self.CANVAS}",
                "love.graphics.setCanvas()",
            ),
            (
                "        love.graphics.draw(self.CANVAS, 0, 0)",
                "        -- The software renderer already drew this frame to the screen.",
            ),
            (
                "love.graphics.setCanvas(G.AA_CANVAS)",
                "love.graphics.setCanvas()",
            ),
            (
                "    if G.AA_CANVAS then \n        love.graphics.push()\n            love.graphics.scale(1/G.CANV_SCALE)\n            love.graphics.draw(G.AA_CANVAS, 0, 0)\n        love.graphics.pop()\n    end",
                "    -- CRT and antialiasing are disabled, so there is no final canvas to composite.",
            ),
            (
                "        for k, v in pairs(self.MOVEABLES) do\n            if v.FRAME.MOVE < G.FRAMES.MOVE then v:move(move_dt) end\n        end\n                    timer_checkpoint('move', 'update')\n        \n        for k, v in pairs(self.MOVEABLES) do\n            v:update(dt*self.SPEEDFACTOR)\n            v.states.collide.is = false\n        end",
                "        SVMM_update_moveables(self, move_dt, dt*self.SPEEDFACTOR)",
            ),
            (
                "for k, v in pairs(self.I.NODE) do",
                "for i = 1, #self.I.NODE do\n        local v = self.I.NODE[i]",
            ),
            (
                "for k, v in pairs(self.I.MOVEABLE) do",
                "for i = 1, #self.I.MOVEABLE do\n        local v = self.I.MOVEABLE[i]",
            ),
            (
                "for k, v in pairs(self.I.UIBOX) do",
                "for i = 1, #self.I.UIBOX do\n            local v = self.I.UIBOX[i]",
            ),
            (
                "for k, v in pairs(self.I.CARDAREA) do",
                "for i = 1, #self.I.CARDAREA do\n            local v = self.I.CARDAREA[i]",
            ),
            (
                "for k, v in pairs(self.I.CARD) do",
                "for i = 1, #self.I.CARD do\n            local v = self.I.CARD[i]",
            ),
            (
                "for k, v in pairs(self.I.ALERT) do",
                "for i = 1, #self.I.ALERT do\n        local v = self.I.ALERT[i]",
            ),
            (
                "for k, v in pairs(self.I.POPUP) do",
                "for i = 1, #self.I.POPUP do\n        local v = self.I.POPUP[i]",
            ),
        ],
        "main.lua" => &[
            (
                "if w/h < G.window_prev.orig_ratio then\n\t\tG.TILESCALE = G.window_prev.orig_scale*w/G.window_prev.w\n\telse\n\t\tG.TILESCALE = G.window_prev.orig_scale*h/G.window_prev.h\n\tend",
                "if G.SVMM_HANDHELD_LAYOUT then\n\t\tG.TILESCALE = w/(G.TILESIZE*G.TILE_W)\n\telseif w/h < G.window_prev.orig_ratio then\n\t\tG.TILESCALE = G.window_prev.orig_scale*w/G.window_prev.w\n\telse\n\t\tG.TILESCALE = G.window_prev.orig_scale*h/G.window_prev.h\n\tend",
            ),
            (
                "if w/h < G.window_prev.orig_ratio then\n\t\t\tG.ROOM.T.x = G.ROOM_PADDING_W\n\t\t\tG.ROOM.T.y = (h/(G.TILESIZE*G.TILESCALE) - (G.ROOM.T.h+G.ROOM_PADDING_H))/2 + G.ROOM_PADDING_H/2\n\t\telse",
                "if G.SVMM_HANDHELD_LAYOUT then\n\t\t\tG.ROOM.T.x = 0\n\t\t\tG.ROOM.T.y = (h/(G.TILESIZE*G.TILESCALE) - G.ROOM.T.h)/2\n\t\telseif w/h < G.window_prev.orig_ratio then\n\t\t\tG.ROOM.T.x = G.ROOM_PADDING_W\n\t\t\tG.ROOM.T.y = (h/(G.TILESIZE*G.TILESCALE) - (G.ROOM.T.h+G.ROOM_PADDING_H))/2 + G.ROOM_PADDING_H/2\n\t\telse",
            ),
        ],
        "functions/common_events.lua" => &[
            (
                "G.hand.T.x = G.TILE_W - G.hand.T.w - 2.85",
                "G.hand.T.x = G.TILE_W - G.hand.T.w - (G.SVMM_HANDHELD_LAYOUT and 2.35 or 2.85)",
            ),
            (
                "G.hand.T.y = G.TILE_H - G.hand.T.h",
                "G.hand.T.y = G.TILE_H - G.hand.T.h - (G.SVMM_HANDHELD_LAYOUT and 1.15 or 0)",
            ),
            (
                "G.deck.T.x = G.TILE_W - G.deck.T.w - 0.5",
                "G.deck.T.x = G.TILE_W - G.deck.T.w - (G.SVMM_HANDHELD_LAYOUT and 0.1 or 0.5)",
            ),
        ],
        "functions/UI_definitions.lua" => &[(
            "minh = 30, padding = 0.08",
            "minh = G.SVMM_HANDHELD_LAYOUT and 0 or 30, padding = 0.08",
        )],
        "engine/moveable.lua" => &[
            (
                "    table.insert(G.MOVEABLES, self)\n    if getmetatable(self) == Moveable then ",
                "    table.insert(G.MOVEABLES, self)\n    local update = self.update\n    local needs_update = update ~= Node.update\n    if needs_update and UIElement and update == UIElement.update then\n        local config = self.config\n        local kind = self.UIT\n        needs_update = kind == G.UIT.O or\n            (kind == G.UIT.T and config.ref_table and config.ref_value) or\n            config.func or config.button_delay\n    end\n    if needs_update then table.insert(G.SVMM_UPDATEABLES, self) end\n    if getmetatable(self) == Moveable then ",
            ),
            (
                "function Moveable:remove()\n    for k, v in pairs(G.MOVEABLES) do",
                "function Moveable:remove()\n    for i = 1, #G.SVMM_UPDATEABLES do\n        if G.SVMM_UPDATEABLES[i] == self then\n            table.remove(G.SVMM_UPDATEABLES, i)\n            break\n        end\n    end\n    for k, v in pairs(G.MOVEABLES) do",
            ),
            (
                "    self.CALCING = nil\n    if self.role.role_type == 'Glued' then",
                "    self.CALCING = nil\n    local velocity = self.velocity\n    local states = self.states\n    if self.role.role_type == 'Major' and self.STATIONARY and\n        not self.NEW_ALIGNMENT and not self.config.refresh_movement and\n        not self.juice and not self.alignment.lr_clamp and\n        not self.pinch.x and not self.pinch.y and\n        not (self.zoom and (states.drag.is or states.hover.is)) and\n        self.T.x == self.VT.x and self.T.y == self.VT.y and\n        self.T.w == self.VT.w and self.T.h == self.VT.h and\n        self.T.r == self.VT.r and self.T.scale == self.VT.scale and\n        velocity.x == 0 and velocity.y == 0 and\n        velocity.r == 0 and velocity.scale == 0 then\n            self.NEW_ALIGNMENT = false\n            return\n    end\n    if self.role.role_type == 'Glued' then",
            ),
            (
                "            self.role.xy_bond == 'Weak' or \n            self.role.r_bond == 'Weak' then  ",
                "            (self.role.xy_bond == 'Weak' and\n                (self.T.x ~= self.VT.x or self.T.y ~= self.VT.y or\n                 math.abs(velocity.x) > 0.01 or math.abs(velocity.y) > 0.01)) or\n            (self.role.r_bond == 'Weak' and\n                (self.T.r ~= self.VT.r or math.abs(velocity.r) > 0.001)) or\n            (self.zoom and (states.drag.is or states.hover.is)) then  ",
            ),
        ],
        "cardarea.lua" => &[
            (
                "self.ARGS.draw_layers = self.ARGS.draw_layers or self.config.draw_layers or {'shadow', 'card'}",
                "self.ARGS.draw_layers = self.ARGS.draw_layers or self.config.draw_layers or {'card'}",
            ),
            (
                "scale = 0.3, colour = G.C.WHITE",
                "scale = 0.5, colour = G.C.WHITE",
            ),
        ],
        "card.lua" => &[
            (
                "    self.children.shadow = Moveable(0, 0, 0, 0)\n",
                "",
            ),
            (
                "if (layer == 'shadow' or layer == 'both') then",
                "if (layer == 'shadow' or layer == 'both' or (layer == 'card' and G.SETTINGS.GRAPHICS.shadows ~= 'On')) then",
            ),
            (
                "function Card:update(dt)\n    if self.flipping == 'f2b' then",
                "function Card:update(dt)\n    if self.ability.set == 'Default' and self.config.center == G.P_CENTERS.c_base and\n        not self.flipping and not self.children.focused_ui and\n        not self.ability.perma_debuff then return end\n    if self.flipping == 'f2b' then",
            ),
        ],
        "engine/ui.lua" => &[
            (
                "function UIElement:remove()\n",
                "function UIElement:remove()\n    if self.config and self.config.text_drawable then\n        self.config.text_drawable:release()\n        self.config.text_drawable = nil\n    end\n",
            ),
            (
                "function UIElement:draw_pixellated_rect(_type, _parallax, _emboss, _progress)",
                "function UIElement:draw_pixellated_rect(_type, _parallax, _emboss, _progress, _native_only)",
            ),
            (
                "    self.config = config or {}\n    if self.config and self.config.object then",
                "    self.config = config or {}\n    self.config.shadow = nil\n    self.config.emboss = nil\n    self.config.line_emboss = nil\n    if self.config.object then",
            ),
            (
                "function UIElement:update(dt)\n    G.ARGS.FUNC_TRACKER = G.ARGS.FUNC_TRACKER or {}\n    if self.config.button_delay then\n        self.config.button_temp = self.config.button or self.config.button_temp\n        self.config.button = nil\n        self.config.button_delay_progress = (G.TIMERS.REAL - self.config.button_delay_start)/self.config.button_delay\n        if G.TIMERS.REAL >= self.config.button_delay_end then self.config.button_delay = nil end\n    end\n    if self.config.button_temp and not self.config.button_delay then self.config.button = self.config.button_temp end\n    if self.button_clicked then self.button_clicked = nil end\n    if self.config and self.config.func then\n        G.ARGS.FUNC_TRACKER[self.config.func] = (G.ARGS.FUNC_TRACKER[self.config.func] or 0) + 1\n        G.FUNCS[self.config.func](self)\n    end\n    if self.UIT == G.UIT.T then self:update_text() end\n    if self.UIT == G.UIT.O then self:update_object() end\n    Node.update(self, dt)\nend",
                "function UIElement:update(dt)\n    local config = self.config\n    if config.button_delay then\n        config.button_temp = config.button or config.button_temp\n        config.button = nil\n        config.button_delay_progress = (G.TIMERS.REAL - config.button_delay_start)/config.button_delay\n        if G.TIMERS.REAL >= config.button_delay_end then config.button_delay = nil end\n    end\n    if config.button_temp and not config.button_delay then config.button = config.button_temp end\n    if self.button_clicked then self.button_clicked = nil end\n    if config.func then G.FUNCS[config.func](self) end\n    local kind = self.UIT\n    if kind == G.UIT.T then self:update_text() end\n    if kind == G.UIT.O then self:update_object() end\nend",
            ),
            (
                "    love.graphics.polygon((_type == 'line' or _type == 'line_emboss') and 'line' or \"fill\", self.pixellated_rect[_type].vertices)\nend",
                "    local polygon = self.pixellated_rect[_type]\n    polygon.native = polygon.native or love.graphics.newPolygonData(polygon.vertices)\n    if _native_only then return polygon.native end\n    love.graphics.drawPolygonData((_type == 'line' or _type == 'line_emboss') and 'line' or \"fill\", polygon.native)\nend",
            ),
        ],
        "engine/text.lua" => &[
            (
                "    self.config = config\n    self.shadow = config.shadow",
                "    self.config = config\n    self.config.shadow = false\n    self.shadow = false",
            ),
            (
                "    self.font = config.font or G.LANG.font",
                "    self.font = config.font or G.LANG.font\n    self.layout_font_scale = self.font.LAYOUT_FONTSCALE or self.font.FONTSCALE",
            ),
            (
                "    self:update_text(true)\n    if self.config.maxw",
                "    self:update_text(true)\n    local first_string = self.config.string[1]\n    self._svmm_can_idle = #self.config.string == 1 and\n        not (type(first_string) == 'table' and first_string.ref_table) and\n        not self.config.float and not self.config.bump and not self.config.rotate and\n        not self.config.pulse and not self.config.quiver\n    if self.config.maxw",
            ),
            (
                "function DynaText:update(dt)\n    self:update_text()\n    self:align_letters()\nend",
                "function DynaText:update(dt)\n    if self._svmm_can_idle and not self.config.pop_in and\n        not self.config.pop_out and not self.config.pulse and\n        not self.config.quiver then return end\n    self:update_text()\n    self:align_letters()\nend",
            ),
            (
                " + 2.7*(self.config.spacing or 0)*G.TILESCALE*self.font.FONTSCALE",
                " + 2.7*(self.config.spacing or 0)*G.TILESCALE*self.layout_font_scale",
            ),
            (
                "(self.font.FONTSCALE/G.TILESIZE)*2000*math.sin",
                "(self.layout_font_scale/G.TILESIZE)*2000*math.sin",
            ),
            (
                "self.text_offset.x*self.font.FONTSCALE/G.TILESIZE",
                "self.text_offset.x*self.layout_font_scale/G.TILESIZE",
            ),
            (
                "self.text_offset.y*self.font.FONTSCALE/G.TILESIZE",
                "self.text_offset.y*self.layout_font_scale/G.TILESIZE",
            ),
            (
                "self.config.spacing*self.font.FONTSCALE/G.TILESIZE",
                "self.config.spacing*self.layout_font_scale/G.TILESIZE",
            ),
            (
                "0.5*(letter.dims.x - letter.offset.x)*self.font.FONTSCALE/G.TILESIZE",
                "0.5*(letter.dims.x*self.font.FONTSCALE - letter.offset.x*self.layout_font_scale)/G.TILESIZE",
            ),
            (
                "0.5*(letter.dims.y - letter.offset.y)*self.font.FONTSCALE/G.TILESIZE",
                "0.5*(letter.dims.y*self.font.FONTSCALE - letter.offset.y*self.layout_font_scale)/G.TILESIZE",
            ),
            (
                "self.shadow_parrallax.x/math.sqrt(self.shadow_parrallax.y*self.shadow_parrallax.y + self.shadow_parrallax.x*self.shadow_parrallax.x)*self.font.FONTSCALE/G.TILESIZE",
                "self.shadow_parrallax.x/math.sqrt(self.shadow_parrallax.y*self.shadow_parrallax.y + self.shadow_parrallax.x*self.shadow_parrallax.x)*self.layout_font_scale/G.TILESIZE",
            ),
            (
                "self.shadow_parrallax.y/math.sqrt(self.shadow_parrallax.y*self.shadow_parrallax.y + self.shadow_parrallax.x*self.shadow_parrallax.x)*self.font.FONTSCALE/G.TILESIZE",
                "self.shadow_parrallax.y/math.sqrt(self.shadow_parrallax.y*self.shadow_parrallax.y + self.shadow_parrallax.x*self.shadow_parrallax.x)*self.layout_font_scale/G.TILESIZE",
            ),
        ],
        "functions/misc_functions.lua" => &[
            ("function PLAY_SOUND(args)", "function PLAY_SOUND(args, defer_play)"),
            ("  love.audio.play(s.sound)", "  if not defer_play then love.audio.play(s.sound) end"),
            (
                "  if obj then \n    G.DRAW_HASH[#G.DRAW_HASH+1] = obj",
                "  if obj then \n    obj.under_overlay = G.under_overlay\n    G.DRAW_HASH[#G.DRAW_HASH+1] = obj",
            ),
            (
                "function modulate_sound(dt)\n  --volume of the splash screen is set here",
                "function modulate_sound(dt)\n  if G.F_MUTE then\n    local intensity = G.ARGS.score_intensity or {}\n    G.ARGS.score_intensity = intensity\n    local hand = G.GAME.current_round.current_hand\n    intensity.earned_score = type(hand.chips) == 'number' and\n      type(hand.mult) == 'number' and hand.chips*hand.mult or 0\n    intensity.required_score = G.GAME.blind and G.GAME.blind.chips or 0\n    return\n  end\n  --volume of the splash screen is set here",
            ),
        ],
        "engine/node.lua" | "engine/particles.lua" | "card_character.lua" => &[],
        "engine/sprite.lua" => &[],
        "engine/event.lua" => &[],
        _ => return,
    };

    let mut script = match String::from_utf8(std::mem::take(data)) {
        Ok(script) => script,
        Err(error) => {
            *data = error.into_bytes();
            return;
        }
    };
    for (from, to) in replacements {
        script = script.replace(from, to);
    }
    if normalized == "engine/event.lua" {
        script.push_str(include_str!("menu_clock.lua"));
    }
    if normalized == "functions/misc_functions.lua" {
        script.push_str(include_str!("music.lua"));
    }
    if normalized == "functions/UI_definitions.lua" {
        script.push_str(include_str!("deck_layout.lua"));
    }
    if normalized == "engine/ui.lua" {
        if let (Some(start), Some(end)) = (
            script.find("function UIElement:draw_self()"),
            script.find("function UIElement:draw_pixellated_rect"),
        ) {
            if start < end {
                script.replace_range(start..end, MIYOO_UI_DRAW_SELF);
            }
        }
    }
    if normalized == "engine/node.lua" {
        script.push_str(crate::miyoo::SCALAR_COLLISION);
    }
    if normalized == "engine/moveable.lua" {
        if let (Some(start), Some(end)) = (
            script.find("function Moveable:move(dt)"),
            script.find("function Moveable:lr_clamp()"),
        ) {
            if start < end {
                script.replace_range(start..end, MIYOO_MOVEABLE_MOVE);
            }
        }
        if let (Some(start), Some(end)) = (
            script.find("function Moveable:move_with_major(dt)"),
            script.find("function Moveable:move_xy(dt)"),
        ) {
            if start < end {
                script.replace_range(start..end, MIYOO_MOVE_WITH_MAJOR);
            }
        }
    }
    if normalized == "engine/sprite.lua" {
        let marker = "function Sprite:draw_shader(_shader, _shadow_height, _send, _no_tilt, other_obj, ms, mr, mx, my, custom_shader, tilt_shadow)\n    if G.SETTINGS.reduced_motion then _no_tilt = true end";
        if let Some(start) = script.find(marker) {
            script.insert_str(start + marker.len(), MIYOO_SPRITE_SHADER_FAST_PATH);
        }
    }
    if normalized == "functions/misc_functions.lua" {
        if let (Some(start), Some(end)) = (
            script.find("function prep_draw(moveable, scale, rotate, offset)"),
            script.find("function get_chosen_triangle_from_rect"),
        ) {
            if start < end {
                script.replace_range(start..end, MIYOO_PREP_DRAW);
            }
        }
    }
    if normalized == "card.lua" {
        let marker = "function Card:draw(layer)\n    layer = layer or 'both'\n\n    self.hover_tilt = 1\n    \n    if not self.states.visible then return end";
        if let Some(start) = script.find(marker) {
            script.insert_str(start, MIYOO_CARD_DRAW_FAST_PATH);
            if let Some(entry_start) = script[start + MIYOO_CARD_DRAW_FAST_PATH.len()..]
                .find(marker)
                .map(|offset| start + MIYOO_CARD_DRAW_FAST_PATH.len() + offset)
            {
                script.replace_range(
                    entry_start..entry_start + marker.len(),
                    MIYOO_CARD_DRAW_ENTRY,
                );
            }
        }
    }
    if normalized == "game.lua"
        && script.contains("SVMM_update_moveables(self, move_dt, dt*self.SPEEDFACTOR)")
    {
        script.insert_str(0, MIYOO_CLASSIFIED_MOVEABLE_SCAN);
    }
    if normalized == "game.lua" {
        let final_pass_start = "love.graphics.pop()\n    \n    love.graphics.setCanvas()";
        let final_pass_end = "    timer_checkpoint('canvas', 'draw')";
        if let Some(start) = script.find(final_pass_start) {
            if let Some(end_offset) = script[start..].find(final_pass_end) {
                let end = start + end_offset;
                script.replace_range(
                    start..end,
                    "love.graphics.pop()\n\n    love.graphics.setShader()\n\n",
                );
            }
        }
    }
    if matches!(
        normalized.as_str(),
        "engine/node.lua"
            | "engine/moveable.lua"
            | "engine/ui.lua"
            | "engine/sprite.lua"
            | "engine/particles.lua"
            | "card.lua"
            | "cardarea.lua"
            | "card_character.lua"
    ) {
        script = script.replace("    self:draw_boundingrect()\n", "");
    }
    *data = script.into_bytes();
}

#[cfg(test)]
#[path = "patch_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "music_tests.rs"]
mod music_tests;
