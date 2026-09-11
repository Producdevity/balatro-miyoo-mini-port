#[test]
fn movement_profile_keeps_inherited_and_subclass_calls() {
    let lua = mlua::Lua::new();
    lua.load(
        r#"
        G = {}
        local time = 0
        love = {timer = {getTime = function() time = time + 0.001; return time end}}
        _SVMM_PROFILE_OWNERS = true
        _SVMM_PROFILE_SAMPLE = true
        Moveable = {move = function(self, dt) self.moved = self.moved + dt end}
        CardArea = setmetatable({
            move = function(self, dt)
                Moveable.move(self, dt)
                self:align_cards()
                return 42
            end,
            align_cards = function(self) self.aligned = self.aligned + 1 end,
        }, {__index = Moveable})
        "#,
    )
    .exec()
    .unwrap();
    lua.load(include_str!("diagnostics/profile.lua"))
        .exec()
        .unwrap();
    lua.load(
        r#"
        local area = setmetatable({role = {role_type = 'Major'}, moved = 0, aligned = 0},
            {__index = CardArea})
        assert(area:move(0.02) == 42)
        assert(area.moved == 0.02 and area.aligned == 1)
        assert(G.SVMM_MOVE_PROFILE['Moveable:Major'].calls == 1)
        assert(G.SVMM_MOVE_PROFILE.CardArea.calls == 1)
        assert(G.SVMM_UPDATE_PROFILE['CardArea.align_cards'].calls == 1)
        _SVMM_PROFILE_SAMPLE = false
        assert(area:move(0.02) == 42)
        assert(area.moved == 0.04 and area.aligned == 2)
        assert(G.SVMM_MOVE_PROFILE.CardArea.calls == 1)
        assert(G.SVMM_UPDATE_PROFILE['CardArea.align_cards'].calls == 1)
        "#,
    )
    .exec()
    .unwrap();
}
