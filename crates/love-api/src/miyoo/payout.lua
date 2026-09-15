local MIN_LABEL_SCALE = 0.6

local function resize_labels(node)
    local object = node.config and node.config.object
    if object and getmetatable(object) == DynaText and
       object.scale < MIN_LABEL_SCALE and not object.config.spacing then
        local config = object.config
        object.scale = MIN_LABEL_SCALE
        config.scale = MIN_LABEL_SCALE
        object.text_offset.x = object.font.TEXT_OFFSET.x*object.scale + (config.x_offset or 0)
        object.text_offset.y = object.font.TEXT_OFFSET.y*object.scale + (config.y_offset or 0)
        object.start_pop_in = config.pop_in
        object:update_text(true)
    end
    for _, child in ipairs(node.nodes or {}) do resize_labels(child) end
end

local original_add_child = UIBox.add_child
function UIBox:add_child(node, parent)
    if self == G.round_eval then
        resize_labels(node)
        local columns = node.nodes
        local money = columns and columns[2] and columns[2].nodes
        local id = money and money[1] and money[1].config.id
        if id and id:match('^dollar_') then
            local left, right = columns[1].config, columns[2].config
            local width = left.minw + right.minw
            left.minw, right.minw = width*0.75, width*0.25
            left.padding, right.padding = 0.02, 0.02
        end
    end
    return original_add_child(self, node, parent)
end

local function fit_background(node)
    if node.config and node.config.minh == 30 then node.config.minh = 0 end
    for _, child in ipairs(node.nodes or {}) do fit_background(child) end
end

local original_create = create_UIBox_round_evaluation
function create_UIBox_round_evaluation()
    local definition = original_create()
    fit_background(definition)
    return definition
end

-- The game's attention-text layer draws after cards, below menus and tooltips.
local original_init = UIBox.init
function UIBox:init(args, ...)
    original_init(self, args, ...)
    if G.round_eval and args.config and args.config.major == G.round_eval then
        self.attention_text = true
        local owner = G.round_eval
        owner._svmm_payout_boxes = owner._svmm_payout_boxes or {}
        table.insert(owner._svmm_payout_boxes, self)
    end
end

-- Cash Out is aligned to the payout screen but is not one of its children.
-- Keep its separate draw pass, while giving it the same lifetime as the screen.
local original_remove = UIBox.remove
function UIBox:remove(...)
    local boxes = self._svmm_payout_boxes
    self._svmm_payout_boxes = nil
    if boxes then
        for i = #boxes, 1, -1 do
            if not boxes[i].REMOVED then boxes[i]:remove() end
        end
    end
    return original_remove(self, ...)
end

local original_update = Game.update_round_eval
function Game:update_round_eval(dt)
    local result = original_update(self, dt)
    if G.round_eval then G.round_eval.attention_text = true end
    return result
end
