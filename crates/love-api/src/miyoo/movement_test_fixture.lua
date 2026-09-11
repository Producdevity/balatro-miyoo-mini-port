Moveable = {_svmm_role_lists = true, move = function() end}
local function item(role, parent, custom)
    return {
        FRAME = {MOVE = 0}, STATIONARY = true,
        role = {role_type = role, major = parent, offset = {x=0, y=0}},
        alignment = {type='cm', prev_type='cm', offset={x=0,y=0}, prev_offset={x=0,y=0}},
        config = {}, pinch = {}, states = {drag={}, hover={}},
        T = {x=0,y=0,w=1,h=1,r=0,scale=1},
        VT = {x=0,y=0,w=1,h=1,r=0,scale=1},
        velocity = {x=0,y=0,r=0,scale=0},
        move = custom and function() error('only collecting in this test') end or Moveable.move,
        _svmm_glued_major = parent,
    }
end
local parent = item('Major')
parent.FRAME.MOVE = 1
G = {MOVEABLES = {}, FRAMES = {MOVE=1}}
for _, role in ipairs({'Major', 'Minor', 'Glued'}) do
    G.MOVEABLES[#G.MOVEABLES+1] = item(role, parent, false)
    G.MOVEABLES[#G.MOVEABLES+1] = item(role, parent, true)
end
