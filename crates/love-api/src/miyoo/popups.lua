local SAFE_MARGIN = 0.2
local POPUP_TEXT_MULT = 1.85
local POPUP_TEXT_MAX = 0.78
local POPUP_MIN_W = 6.1

local function enlarge_description_text(node)
    if type(node) ~= 'table' then return end
    if node.n == G.UIT.T and node.config and node.config.scale then
        node.config.scale = math.min(POPUP_TEXT_MAX,
            node.config.scale*POPUP_TEXT_MULT)
    elseif node.n == G.UIT.O and node.config and node.config.object and
           type(node.config.object.scale) == 'number' then
        local object = node.config.object
        object.scale = math.min(POPUP_TEXT_MAX,
            object.scale*POPUP_TEXT_MULT)
        if object.update_text then
            local pop_in, pop_out = object.config.pop_in, object.config.pop_out
            local created_time = object.created_time
            object:update_text(true)
            if object.config.maxw and object.config.W > object.config.maxw then
                object.scale = object.scale*object.config.maxw/object.config.W
                object:update_text(true)
            end
            object.config.pop_in, object.config.pop_out = pop_in, pop_out
            object.created_time = created_time
        end
    end
    if node.nodes then
        for _, child in ipairs(node.nodes) do enlarge_description_text(child) end
    elseif not node.config then
        for _, child in ipairs(node) do enlarge_description_text(child) end
        for _, key in ipairs({'main', 'info', 'name', 'type'}) do
            if node[key] then enlarge_description_text(node[key]) end
        end
    end
end

local original_ability_table = Card.generate_UIBox_ability_table
function Card:generate_UIBox_ability_table(...)
    local result = original_ability_table(self, ...)
    enlarge_description_text(result)
    return result
end

local function overlap(x, y, w, h, other)
    if not other then return 0 end
    return math.max(0, math.min(x+w, other.x+other.w)-math.max(x, other.x))*
           math.max(0, math.min(y+h, other.y+other.h)-math.max(y, other.y))
end

local function popup_position(box, owner, visual)
    local t = visual and box.VT or box.T
    local a = owner and (visual and owner.VT or owner.T)
    local hud = G.STAGE == G.STAGES.RUN and G.HUD
    local top = hud and (hud.T.y + hud.T.h + 0.08) or SAFE_MARGIN
    local max_x = math.max(SAFE_MARGIN, G.ROOM.T.w-t.w-SAFE_MARGIN)
    local max_y = math.max(top, G.ROOM.T.h-t.h-SAFE_MARGIN)
    local function clamp(x, y)
        return math.min(math.max(x, SAFE_MARGIN), max_x),
               math.min(math.max(y, top), max_y)
    end
    if not a then return clamp(t.x, t.y) end

    local area = owner.area and (visual and owner.area.VT or owner.area.T)
    local best_x, best_y, best_score
    local function consider(x, y)
        x, y = clamp(x, y)
        -- Keep the focused card visible first, then preserve the rest of its row.
        local score = overlap(x,y,t.w,t.h,a)*1000 + overlap(x,y,t.w,t.h,area)
        score = score + (math.abs(x-t.x) + math.abs(y-t.y))*0.001
        if not best_score or score < best_score then
            best_x, best_y, best_score = x, y, score
        end
    end
    local centred = a.x + (a.w-t.w)/2
    if area then
        consider(centred, area.y-t.h-0.15)
        consider(centred, area.y+area.h+0.15)
    end
    consider(centred, a.y-t.h-0.15)
    consider(centred, a.y+a.h+0.15)
    consider(a.x-t.w-0.15, a.y+(a.h-t.h)/2)
    consider(a.x+a.w+0.15, a.y+(a.h-t.h)/2)
    return best_x, best_y
end

local function clamp_popup_to_room(box)
    local owner = box.parent
    if not (owner and getmetatable(owner) == Card) then owner = nil end
    box.T.x, box.T.y = popup_position(box, owner, false)
    box.VT.x, box.VT.y = popup_position(box, owner, true)
end

local original_uibox_draw = UIBox.draw
function UIBox:draw(...)
    if G.deck_preview and self ~= G.deck_preview and
       self.config.instance_type == 'POPUP' then return end
    return original_uibox_draw(self, ...)
end

local original_uibox_move = UIBox.move
function UIBox:move(dt)
    if self.config and self.config.instance_type == 'POPUP' then
        Moveable.move(self, dt)
        clamp_popup_to_room(self)
        Moveable.move(self.UIRoot, dt)
        return
    end
    return original_uibox_move(self, dt)
end

local function stack_info_boxes(node)
    if type(node) ~= 'table' then return false end
    if node.config and node.config.func == 'show_infotip' and
       type(node.config.ref_table) == 'table' and node.nodes then
        for _, box in ipairs(node.config.ref_table) do
            node.nodes[#node.nodes + 1] = box
        end
        node.config.ref_table = nil
        return true
    end
    if node.nodes then
        for _, child in pairs(node.nodes) do
            if stack_info_boxes(child) then return true end
        end
    end
    return false
end

local original_card_h_popup = G.UIDEF.card_h_popup
G.UIDEF.card_h_popup = function(...)
    local definition = original_card_h_popup(...)
    if type(definition) == 'table' then
        pcall(stack_info_boxes, definition)
        definition.config.minw = math.max(definition.config.minw or 0, POPUP_MIN_W)
        local content = definition.nodes and definition.nodes[1]
        if content and content.config then
            content.config.minw = math.max(content.config.minw or 0, POPUP_MIN_W)
        end
    end
    return definition
end

local original_align_h_popup = Card.align_h_popup
function Card:align_h_popup(...)
    local result = original_align_h_popup(self, ...)
    result.lr_clamp = true
    return result
end
