local SVMM_ui_draws = {}
local SVMM_dynamic_ui_draws = {}
local SVMM_ui_draw_count = 0
local SVMM_dynamic_ui_draw_count = 0
local SVMM_static_ui_cache = os.getenv('BALATRO_STATIC_UI_CACHE') ~= '0'

function SVMM_flush_ui_draws()
    if SVMM_ui_draw_count == 0 then return end
    love.graphics._drawUIBatch(SVMM_ui_draws, SVMM_ui_draw_count)
    for i = 1, SVMM_ui_draw_count do SVMM_ui_draws[i] = nil end
    SVMM_ui_draw_count = 0
end

local function SVMM_queue_ui_draw(draw)
    SVMM_ui_draw_count = SVMM_ui_draw_count + 1
    SVMM_ui_draws[SVMM_ui_draw_count] = draw
end

local function SVMM_next_ui_draw()
    SVMM_dynamic_ui_draw_count = SVMM_dynamic_ui_draw_count + 1
    local draw = SVMM_dynamic_ui_draws[SVMM_dynamic_ui_draw_count]
    if not draw then
        draw = {}
        SVMM_dynamic_ui_draws[SVMM_dynamic_ui_draw_count] = draw
    end
    SVMM_queue_ui_draw(draw)
    return draw
end

local function SVMM_normalize_ui_colour(element, colour)
    if type(colour) == 'table' then return colour end
    G._svmm_invalid_ui_colours = (G._svmm_invalid_ui_colours or 0) + 1
    if not G._svmm_reported_invalid_ui_colour then
        G._svmm_reported_invalid_ui_colour = true
        print('[ui] normalized invalid colour type='..type(colour))
    end
    local configured = element.config and element.config.colour
    if type(configured) == 'table' then return configured end
    return G.C.WHITE
end

local function SVMM_draw_ui_polygon(element, kind, parallax, emboss, progress, colour, local_scale, line_width, cache_static)
    colour = SVMM_normalize_ui_colour(element, colour)
    local visible = element.VT
    local layered = element.layered_parallax
    local parent = element.parent
    local parent_layered = parent and parent.layered_parallax
    local parallax_x = (layered and layered.x) or (parent_layered and parent_layered.x) or 0
    local parallax_y = (layered and layered.y) or (parent_layered and parent_layered.y) or 0
    local shadow = element.shadow_parrallax
    local shadow_x, shadow_y = shadow.x, shadow.y
    local draw
    if SVMM_static_ui_cache and cache_static and element.STATIONARY then
        draw = element._svmm_polygon_draw
        local shape_unchanged = draw and draw._kind == kind and
            draw._shape_w == visible.w and draw._shape_h == visible.h and
            draw._shadow_x == shadow_x and draw._shadow_y == shadow_y and
            draw._parallax == parallax and draw._emboss == emboss and
            draw._progress == progress
        local polygon = shape_unchanged and draw[2] or
            element:draw_pixellated_rect(kind, parallax, emboss, progress, true)
        if not polygon then return end
        if shape_unchanged and draw._x == visible.x and
            draw._y == visible.y and draw._w == visible.w and draw._h == visible.h and
            draw._rotation == visible.r and draw._scale == visible.scale and
            draw._parallax_x == parallax_x and draw._parallax_y == parallax_y and
            draw._local_scale == local_scale and draw._line_width == (line_width or 1) and
            draw._r == colour[1] and draw._g == colour[2] and
            draw._b == colour[3] and draw._a == colour[4] then
                SVMM_queue_ui_draw(draw)
                return
        end
        draw = draw or {}
        element._svmm_polygon_draw = draw
        draw._svmm_static = true
        draw._native_ui_dirty = true
        draw[2] = polygon
        draw._kind, draw._shape_w, draw._shape_h = kind, visible.w, visible.h
        draw._shadow_x, draw._shadow_y = shadow_x, shadow_y
        draw._parallax, draw._emboss, draw._progress = parallax, emboss, progress
        draw._x, draw._y, draw._w, draw._h = visible.x, visible.y, visible.w, visible.h
        draw._rotation, draw._scale = visible.r, visible.scale
        draw._parallax_x, draw._parallax_y = parallax_x, parallax_y
        draw._local_scale, draw._line_width = local_scale, line_width or 1
        draw._r, draw._g, draw._b, draw._a = colour[1], colour[2], colour[3], colour[4]
        SVMM_queue_ui_draw(draw)
    else
        if cache_static then element._svmm_polygon_draw = nil end
        draw = SVMM_next_ui_draw()
        local polygon = element:draw_pixellated_rect(kind, parallax, emboss, progress, true)
        if not polygon then return end
        draw[2] = polygon
    end
    draw[1], draw[3] = 0, kind ~= 'line' and kind ~= 'line_emboss'
    draw[4], draw[5], draw[6] = G.TILESCALE*G.TILESIZE,
        visible.x+visible.w/2+parallax_x, visible.y+visible.h/2+parallax_y
    draw[7], draw[8], draw[9] = visible.r,
        -visible.w*visible.scale/2, -visible.h*visible.scale/2
    draw[10], draw[11] = visible.scale, local_scale/G.TILESIZE
    draw[12], draw[13], draw[14], draw[15] = colour[1], colour[2], colour[3], colour[4]
    draw[16] = line_width or 1
end

local function SVMM_draw_ui_text(element, drawable, colour, local_x, local_y, scale_x, scale_y, vertical, cache_static)
    local visible = element.VT
    local layered = element.layered_parallax
    local parent = element.parent
    local parent_layered = parent and parent.layered_parallax
    local parallax_x = (layered and layered.x) or (parent_layered and parent_layered.x) or 0
    local parallax_y = (layered and layered.y) or (parent_layered and parent_layered.y) or 0
    local draw
    if SVMM_static_ui_cache and cache_static and element.STATIONARY then
        draw = element._svmm_text_draw
        if draw and draw[2] == drawable and draw._x == visible.x and
            draw._y == visible.y and draw._w == visible.w and draw._h == visible.h and
            draw._rotation == visible.r and draw._scale == visible.scale and
            draw._parallax_x == parallax_x and draw._parallax_y == parallax_y and
            draw._local_x == local_x and draw._local_y == local_y and
            draw._scale_x == scale_x and draw._scale_y == scale_y and
            draw._vertical == not not vertical and draw._r == colour[1] and
            draw._g == colour[2] and draw._b == colour[3] and draw._a == colour[4] then
                SVMM_queue_ui_draw(draw)
                return
        end
        draw = draw or {}
        element._svmm_text_draw = draw
        draw._svmm_static = true
        draw._native_ui_dirty = true
        draw._x, draw._y, draw._w, draw._h = visible.x, visible.y, visible.w, visible.h
        draw._rotation, draw._scale = visible.r, visible.scale
        draw._parallax_x, draw._parallax_y = parallax_x, parallax_y
        draw._local_x, draw._local_y = local_x, local_y
        draw._scale_x, draw._scale_y, draw._vertical = scale_x, scale_y, not not vertical
        draw._r, draw._g, draw._b, draw._a = colour[1], colour[2], colour[3], colour[4]
        SVMM_queue_ui_draw(draw)
    else
        if cache_static then element._svmm_text_draw = nil end
        draw = SVMM_next_ui_draw()
    end
    draw[1], draw[2], draw[3] = 1, drawable, not not vertical
    draw[4], draw[5], draw[6] = G.TILESCALE*G.TILESIZE,
        visible.x+visible.w/2+parallax_x, visible.y+visible.h/2+parallax_y
    draw[7], draw[8], draw[9] = visible.r,
        -visible.w*visible.scale/2, -visible.h*visible.scale/2
    draw[10], draw[11] = visible.scale, visible.h
    draw[12], draw[13], draw[14], draw[15] = local_x, local_y, scale_x, scale_y
    draw[16], draw[17], draw[18], draw[19] = colour[1], colour[2], colour[3], colour[4]
end

local function SVMM_plain_ui_kind(element, config, states)
    if config.force_focus or config.force_collision or config.button_UIE or
        config.button or config.func or config.button_delay or
        config.progress_bar or config.outline or config.chosen or
        config.focus_args or config.choice or states.collide.can then
            return 0
    end

    local parent = element.parent
    while parent and parent ~= element.UIBox do
        local parent_config = parent.config
        if parent_config and (parent_config.func or parent_config.button or
            parent_config.button_UIE or parent_config.focus_args or
            parent_config.choice) then
                return 0
        end
        parent = parent.parent
    end

    local kind = element.UIT
    if kind == G.UIT.T and config.scale and
        not config.ref_table and not config.ref_value then
            return 1
    end
    if (kind == G.UIT.B or kind == G.UIT.C or kind == G.UIT.R or
        kind == G.UIT.ROOT) and config.r then
            return 2
    end
    return 0
end

function UIElement:draw_self()
    local config = self.config
    local states = self.states
    if not states.visible then
        if config.force_focus then add_to_drawhash(self) end
        return
    end

    local plain_kind = self._svmm_plain_ui
    if plain_kind == nil then
        plain_kind = SVMM_plain_ui_kind(self, config, states)
        self._svmm_plain_ui = plain_kind
    end
    if plain_kind ~= 0 and (config.force_focus or config.force_collision or
        config.button_UIE or config.button or config.func or config.button_delay or
        config.progress_bar or config.outline or config.chosen or config.focus_args or
        config.choice or states.collide.can or states.focus.is or
        (plain_kind == 1 and (config.ref_table or config.ref_value))) then
            plain_kind = 0
            self._svmm_plain_ui = 0
    end
    if plain_kind == 1 then
        if config.colour[4] > 0.01 then
            if not config.text_drawable then UIElement.update_text(self) end
            local font = config.lang.font
            local layout_font_scale = font.LAYOUT_FONTSCALE or font.FONTSCALE
            local scale = config.scale
            SVMM_draw_ui_text(
                self,
                config.text_drawable,
                config.colour,
                font.TEXT_OFFSET.x*scale*layout_font_scale/G.TILESIZE,
                font.TEXT_OFFSET.y*scale*layout_font_scale/G.TILESIZE,
                scale*font.squish*font.FONTSCALE/G.TILESIZE,
                scale*font.FONTSCALE/G.TILESIZE,
                config.vert,
                true)
        end
        self.focus_timer = nil
        return
    elseif plain_kind == 2 then
        if config.colour[4] > 0.01 and self.VT.w > 0.01 then
            SVMM_draw_ui_polygon(self, 'fill', 1.5, nil, nil, config.colour, 1, nil, true)
        end
        self.focus_timer = nil
        return
    end

    if config.force_focus or config.force_collision or config.button_UIE or
        config.button or states.collide.can then
        add_to_drawhash(self)
    end

    local button_active = true
    local parallax_dist = 1.5
    local button_pressed = false
    local tile_size = G.TILESIZE
    local graphics = love.graphics
    local button = config.button
    local button_uie = config.button_UIE
    local colour = config.colour
    local kind = self.UIT
    local uit = G.UIT
    local visible = self.VT

    if not button and not button_uie and not states.focus.is and
        not config.outline and not config.chosen and colour[4] > 0.01 then
        if kind == uit.T and config.scale then
            if not config.text_drawable then UIElement.update_text(self) end
            local font = config.lang.font
            local layout_font_scale = font.LAYOUT_FONTSCALE or font.FONTSCALE
            local scale = config.scale
            SVMM_draw_ui_text(
                self,
                config.text_drawable,
                colour,
                font.TEXT_OFFSET.x*scale*layout_font_scale/tile_size,
                font.TEXT_OFFSET.y*scale*layout_font_scale/tile_size,
                scale*font.squish*font.FONTSCALE/tile_size,
                scale*font.FONTSCALE/tile_size,
                config.vert)
            self.focus_timer = nil
            return
        elseif (kind == uit.B or kind == uit.C or kind == uit.R or kind == uit.ROOT) and
            config.r and visible.w > 0.01 and
            not config.button_delay and not config.progress_bar then
                SVMM_draw_ui_polygon(self, 'fill', 1.5, nil, nil, colour, 1)
                self.focus_timer = nil
                return
        end
    end

    if button or button_uie then
        local parent = self.parent
        local parent_parallax = parent and parent ~= self.UIBox and parent.layered_parallax
        self.layered_parallax.x = parent_parallax and parent_parallax.x or 0
        self.layered_parallax.y = parent_parallax and parent_parallax.y or 0

        if button and ((self.last_clicked and self.last_clicked > G.TIMERS.REAL - 0.1) or
            ((states.hover.is or states.drag.is) and G.CONTROLLER.is_cursor_down)) then
            local distance = config.button_dist or 1
            self.layered_parallax.x = self.layered_parallax.x -
                parallax_dist*self.shadow_parrallax.x/tile_size*distance
            self.layered_parallax.y = self.layered_parallax.y -
                parallax_dist*self.shadow_parrallax.y/tile_size*distance
            parallax_dist = 0
            button_pressed = true
        end

        if button_uie and not button_uie.config.button then button_active = false end
    end

    if colour[4] > 0.01 then
        if kind == uit.T and config.scale then
            if not config.text_drawable then UIElement.update_text(self) end
            local font = config.lang.font
            local layout_font_scale = font.LAYOUT_FONTSCALE or font.FONTSCALE
            local scale = config.scale
            SVMM_draw_ui_text(
                self,
                config.text_drawable,
                button_active and colour or G.C.UI.TEXT_INACTIVE,
                font.TEXT_OFFSET.x*scale*layout_font_scale/tile_size,
                font.TEXT_OFFSET.y*scale*layout_font_scale/tile_size,
                scale*font.squish*font.FONTSCALE/tile_size,
                scale*font.FONTSCALE/tile_size,
                config.vert)
        elseif kind == uit.B or kind == uit.C or kind == uit.R or kind == uit.ROOT then
            local collided_button = button_uie or self
            local button_colours = self.ARGS.button_colours or {}
            self.ARGS.button_colours = button_colours
            button_colours[1] = config.button_delay and
                mix_colours(colour, G.C.L_BLACK, 0.5) or colour
            button_colours[2] = (((collided_button.config.hover and
                collided_button.states.hover.is) or
                (collided_button.last_clicked and
                collided_button.last_clicked > G.TIMERS.REAL - 0.1)) and G.C.UI.HOVER or nil)

            for i = 1, #button_colours do
                local draw_colour = button_colours[i]
                if config.r and visible.w > 0.01 then
                    local local_scale = button_pressed and 0.985 or 1
                    if config.button_delay then
                        SVMM_draw_ui_polygon(self, 'fill', parallax_dist, nil, nil,
                            G.C.GREY, local_scale)
                        SVMM_draw_ui_polygon(self, 'fill', parallax_dist, nil,
                            config.button_delay_progress, draw_colour, local_scale)
                    elseif config.progress_bar then
                        local progress = config.progress_bar
                        SVMM_draw_ui_polygon(self, 'fill', parallax_dist, nil, nil,
                            progress.empty_col or G.C.GREY, local_scale)
                        SVMM_draw_ui_polygon(self, 'fill', parallax_dist, nil,
                            progress.ref_table[progress.ref_value]/progress.max,
                            progress.filled_col or G.C.BLUE, local_scale)
                    else
                        SVMM_draw_ui_polygon(self, 'fill', parallax_dist, nil, nil,
                            draw_colour, local_scale)
                    end
                else
                    SVMM_flush_ui_draws()
                    prep_draw(self, 1)
                    graphics.scale(1/tile_size)
                    graphics.scale(button_pressed and 0.985 or 1)
                    graphics.setColor(draw_colour)
                    graphics.rectangle('fill', 0, 0,
                        visible.w*tile_size, visible.h*tile_size)
                    graphics.pop()
                end
            end
        elseif kind == uit.O and config.object then
            SVMM_flush_ui_draws()
            if config.focus_with_object and config.object.states.focus.is then
                self.object_focus_timer = self.object_focus_timer or G.TIMERS.REAL
                local line_width = 50*math.max(0,
                    self.object_focus_timer - G.TIMERS.REAL + 0.3)^2
                SVMM_draw_ui_polygon(self, 'fill', parallax_dist, nil, nil,
                    adjust_alpha(G.C.WHITE, 0.2*line_width, true), 1, line_width + 1.5)
                SVMM_draw_ui_polygon(self, 'line', parallax_dist, nil, nil,
                    colour[4] > 0 and mix_colours(G.C.WHITE, colour, 0.8) or G.C.WHITE,
                    1, line_width + 1.5)
            else
                self.object_focus_timer = nil
            end
            SVMM_flush_ui_draws()
            config.object:draw()
        end
    end

    local outline_colour = config.outline_colour
    if config.outline and outline_colour[4] > 0.01 then
        if config.r and visible.w > 0.01 then
            SVMM_draw_ui_polygon(self, 'line', parallax_dist, nil, nil,
                outline_colour, 1, config.outline)
        else
            SVMM_flush_ui_draws()
            prep_draw(self, 1)
            graphics.scale(1/tile_size)
            graphics.setLineWidth(config.outline)
            graphics.setColor(outline_colour)
            graphics.rectangle('line', 0, 0, visible.w*tile_size, visible.h*tile_size)
            graphics.pop()
        end
    end

    if states.focus.is then
        self.focus_timer = self.focus_timer or G.TIMERS.REAL
        local line_width = 50*math.max(0, self.focus_timer - G.TIMERS.REAL + 0.3)^2
        SVMM_draw_ui_polygon(self, 'fill', parallax_dist, nil, nil,
            adjust_alpha(G.C.WHITE, 0.2*line_width, true), 1, line_width + 1.5)
        SVMM_draw_ui_polygon(self, 'line', parallax_dist, nil, nil,
            colour[4] > 0 and mix_colours(G.C.WHITE, colour, 0.8) or G.C.WHITE,
            1, line_width + 1.5)
    else
        self.focus_timer = nil
    end

    if config.chosen then
        SVMM_flush_ui_draws()
        prep_draw(self, 1)
        graphics.scale(1/tile_size)
        graphics.setColor(G.C.RED)
        graphics.polygon('fill', get_chosen_triangle_from_rect(
            self.layered_parallax.x, self.layered_parallax.y,
            visible.w*tile_size, visible.h*tile_size, config.chosen == 'vert'))
        graphics.pop()
    end
    self:draw_boundingrect()
end

local function SVMM_draw_ui_children(node)
    if not node.states.visible then return end
    local children = node.children
    for key, child in pairs(children) do
        local config = child.config
        if not config.draw_layer and key ~= 'h_popup' and key ~= 'alert' then
            local draw_self = child.draw_self
            if draw_self == UIElement.draw_self then
                if not config.draw_after then draw_self(child) end
                SVMM_draw_ui_children(child)
                if config.draw_after then draw_self(child) end
            else
                SVMM_flush_ui_draws()
                if draw_self and not config.draw_after then draw_self(child) else child:draw() end
                local draw_children = child.draw_children
                if draw_children then draw_children(child) end
                SVMM_flush_ui_draws()
                if draw_self and config.draw_after then draw_self(child) else child:draw() end
            end
        end
    end
end

function UIElement:draw_children()
    SVMM_dynamic_ui_draw_count = 0
    SVMM_draw_ui_children(self)
    SVMM_flush_ui_draws()
end

SVMM_UI_JIT_FUNCTIONS = {
    draw_polygon = SVMM_draw_ui_polygon,
    draw_text = SVMM_draw_ui_text,
    draw_children = SVMM_draw_ui_children,
    plain_kind = SVMM_plain_ui_kind,
    normalize_colour = SVMM_normalize_ui_colour,
}
