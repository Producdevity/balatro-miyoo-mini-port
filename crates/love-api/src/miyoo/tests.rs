use super::SCALAR_COLLISION;
use mlua::Lua;
use std::io::Read;

#[test]
#[ignore = "requires user-owned game archive in BALATRO_TEST_GAME"]
fn constrained_ui_layout_registers_each_element_once() {
    let path = std::env::var("BALATRO_TEST_GAME").expect("set BALATRO_TEST_GAME");
    let mut archive = zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
    let mut source = Vec::new();
    archive
        .by_name("engine/ui.lua")
        .unwrap()
        .read_to_end(&mut source)
        .unwrap();
    super::patches::patch_miyoo_script("engine/ui.lua", &mut source);
    let lua = Lua::new();
    lua.load(
        r#"
        Moveable = {extend=function() return {} end, init=function(self,args)
            table.insert(G.MOVEABLES,self)
            self.T = {x=args.T.x,y=args.T.y,w=args.T.w,h=args.T.h}
            self.states = {click={},drag={},collide={},hover={}}
        end}
        G = {MOVEABLES={},UIT={T=1,B=2,O=3,R=4,C=5,ROOT=6,padding=0},
            TILESIZE=1,TILESCALE=1,C={UI={},CLEAR={}},FUNCS={}}
        G.LANG = {font={squish=1,FONTSCALE=1,TEXT_HEIGHT_SCALE=1,
            FONT={getWidth=function(_,text) return #text end,getHeight=function() return 1 end}}}
    "#,
    )
    .exec()
    .unwrap();
    lua.load(&source).exec().unwrap();
    lua.load(
        r#"
        UIElement.__index = UIElement
        function UIElement:set_role(role) self.role=role end
        local box = setmetatable({draw_layers={}},{__index=UIBox})
        local function element(kind,config,children)
            return setmetatable({UIT=kind,config=config,children=children or {},
                ARGS={},UIBox=box},UIElement)
        end
        for iteration=1,100 do
            G.MOVEABLES = {}
            local text = element(G.UIT.T,{text='Cash Out',scale=1})
            local row = element(G.UIT.R,{maxw=4},{text})
            local root = element(G.UIT.ROOT,{maxw=2},{row})
            local bounds = {x=0,y=0,w=0,h=0}
            UIBox.calculate_xywh(box,root,bounds)
            assert(#G.MOVEABLES==3,'layout registered an element more than once')
            assert(math.abs(root.T.w-2)<0.00001,'constrained width changed')
            local states = text.states
            for pass=1,3 do UIBox.calculate_xywh(box,root,bounds,true) end
            assert(#G.MOVEABLES==3,'recalculation registered another element')
            assert(text.states==states,'recalculation reset interaction state')
            local added = element(G.UIT.T,{text='New',scale=0.25})
            row.children[2]=added
            UIBox.calculate_xywh(box,root,bounds,true)
            assert(#G.MOVEABLES==4,'new element was not initialized during recalculation')
            assert(added.T and added.states,'new element is missing its movement state')
        end
    "#,
    )
    .exec()
    .unwrap();
}

#[test]
fn blind_panel_position_uses_its_measured_height() {
    let lua = Lua::new();
    lua.load("Game = {update_blind_select=function() end}; G = {}")
        .exec()
        .unwrap();
    let source = crate::state::MIYOO_SMALL_SCREEN_PATCH;
    let section = &source[source.find("local original_update_blind_select =").unwrap()
        ..source.find("local original_notify_alert =").unwrap()];
    lua.load(format!(
        "local ROOM_H, SAFE_MARGIN, RUN_OVERLAY_Y = 12.8, 0.2, 5.35\n\
         local function pin_run_overlay(box,y) box.T.y=y end\n{section}"
    ))
    .exec()
    .unwrap();
    lua.load(
        r#"
        for _, height in ipairs({6, 7.25, 9.2}) do
            G.blind_select = {T={h=height}, config={}}
            Game:update_blind_select(0.016)
            local box = G.blind_select
            assert(box.T.y+box.T.h <= 12.6+0.00001, 'panel extends below the screen')
            assert(box.T.y >= 2.3, 'panel covers the HUD')
            assert(box.config.draw_after_cards, 'panel uses the background draw pass')
        end
        G.blind_select = nil
        Game:update_blind_select(0.016)
        "#,
    )
    .exec()
    .unwrap();
}

#[test]
fn compact_hand_applies_one_bottom_inset_and_keeps_the_scoring_slide() {
    let lua = Lua::new();
    lua.load(
        r#"
        G = {TILE_H=12.8, hand={T={h=3.267}}, selecting=true}
        CardArea = {move=function(self)
            self.T.y = G.TILE_H - self.T.h - (G.selecting and 1.9 or 0)
        end}
        "#,
    )
    .exec()
    .unwrap();
    let source = crate::state::MIYOO_SMALL_SCREEN_PATCH;
    let section = &source[source.find("local original_cardarea_move =").unwrap()
        ..source.find("local original_cardarea_draw =").unwrap()];
    lua.load(format!(
        "local BOTTOM_INSET=2.35\nlocal function apply_hand_card_scale() end\n{section}"
    ))
    .exec()
    .unwrap();
    lua.load(
        r#"
        CardArea.move(G.hand, 0.1)
        local selecting_y = G.hand.T.y
        assert(math.abs(selecting_y+G.hand.T.h-(12.8-2.35)) < 0.001)
        assert(G.TILE_H == 12.8, 'hand movement changed the room height')
        G.selecting = false
        CardArea.move(G.hand, 0.1)
        assert(math.abs(G.hand.T.y-selecting_y-1.9) < 0.001,
            'hand no longer slides down while scoring')
        local other = {T={h=2.75}}
        CardArea.move(other, 0.1)
        assert(math.abs(other.T.y-(12.8-2.75)) < 0.001,
            'compact hand positioning changed other card areas')
        "#,
    )
    .exec()
    .unwrap();
}

#[test]
fn popup_placement_preserves_the_focused_card_and_rebuilds_text_metrics() {
    let lua = Lua::new();
    lua.load(
        r#"
        Card = {generate_UIBox_ability_table=function(self) return self.definition end,
            align_h_popup=function() return {} end}
        UIBox = {move=function() end, draw=function() end}
        Moveable = {move=function() end}
        G = {STAGE=1,STAGES={RUN=1},ROOM={T={w=17.0667,h=12.8}},
            HUD={T={y=0.2,h=2.08}},UIT={T=1,O=2},UIDEF={card_h_popup=function() end}}
    "#,
    )
    .exec()
    .unwrap();
    let script = format!(
        "{}\nreturn popup_position, overlap",
        include_str!("popups.lua")
    );
    let (place, overlap): (mlua::Function, mlua::Function) = lua.load(script).eval().unwrap();
    lua.globals().set("place", place).unwrap();
    lua.globals().set("overlap", overlap).unwrap();
    lua.load(
        r#"
        for _, t in ipairs({{x=6,y=8,w=1.5,h=2.4},{x=0.2,y=2.5,w=2,h=2.8},
            {x=14,y=6,w=2,h=2.8},{x=7,y=6,w=2,h=2.8}}) do
            local owner={T=t,VT=t,area={T={x=0.2,y=t.y,w=14,h=t.h}}}
            local box={T={x=0,y=0,w=6.1,h=3.5}}
            local x,y=place(box,owner,false)
            assert(overlap(x,y,6.1,3.5,t)==0, 'focused card was covered')
            assert(x>=0.2 and x+6.1<=16.87 and y>=2.36 and y+3.5<=12.6)
        end
        local object={scale=0.4,config={W=4,maxw=6},update_text=function(self,first)
            assert(first, 'text metrics were not rebuilt')
            self.config.W=self.scale*10
        end}
        Card.generate_UIBox_ability_table({definition={n=2,config={object=object}}})
        assert(math.abs(object.config.W-6)<0.001 and math.abs(object.scale-0.6)<0.001)
    "#,
    )
    .exec()
    .unwrap();
}

#[test]
fn update_callbacks_see_previous_collisions_before_the_next_controller_pass() {
    let lua = Lua::new();
    lua.load(
        r#"
        local button = {states={collide={is=true}}, update=function(self)
            assert(self.states.collide.is, 'hover callback lost its previous collision')
        end}
        local parent = {states={collide={is=true}}}
        local dragged = {states={collide={is=true}}}
        G = {MOVEABLES={}, FRAMES={MOVE=1}, SVMM_UPDATEABLES={button},
            CONTROLLER={collision_list={button,parent}, dragging={target=dragged}}}
        function timer_checkpoint() end
    "#,
    )
    .exec()
    .unwrap();
    lua.load(include_str!("classified_moveables.lua"))
        .exec()
        .unwrap();
    lua.load(
        r#"
        SVMM_update_moveables(G, 1/30, 1/30)
        for _, item in ipairs(G.CONTROLLER.collision_list) do
            assert(not item.states.collide.is, 'collision remained set before controller update')
        end
        assert(not G.CONTROLLER.dragging.target.states.collide.is)
    "#,
    )
    .exec()
    .unwrap();
}

#[test]
fn cached_plain_panel_still_draws_its_focus_outline() {
    let lua = Lua::new();
    lua.load(
        r#"
        UIElement = {draw_pixellated_rect = function() return {} end,
            draw_boundingrect = function() end}
        draws = 0
        love = {graphics = {_drawUIBatch = function(_, count) draws = draws + count end}}
        G = {UIT = {T=1,B=2,C=3,R=4,ROOT=5}, TILESIZE=20,TILESCALE=1,
            TIMERS={REAL=1}, C={WHITE={1,1,1,1},UI={}}}
        function adjust_alpha(c) return c end
        function mix_colours(a) return a end
        panel = setmetatable({config={r=0.1,colour={0.5,0.5,0.5,1}},
            UIT=G.UIT.C, UIBox={}, ARGS={}, STATIONARY=true,
            states={visible=true,focus={is=false},collide={can=false}},
            VT={x=0,y=0,w=2,h=1,r=0,scale=1},
            shadow_parrallax={x=0,y=0}}, {__index=UIElement})
    "#,
    )
    .exec()
    .unwrap();
    lua.load(include_str!("ui_draw.lua")).exec().unwrap();
    lua.load(
        r#"
        panel:draw_self()
        SVMM_flush_ui_draws()
        assert(draws == 1 and panel._svmm_plain_ui == 2)
        draws = 0
        panel.states.focus.is = true
        panel:draw_self()
        SVMM_flush_ui_draws()
        assert(draws == 3, 'cached panel suppressed its focus fill and outline')
    "#,
    )
    .exec()
    .unwrap();
}

#[test]
fn deck_overview_fits_card_rows_without_resizing_faces() {
    let lua = Lua::new();
    lua.load(
        r#"
        local card = {T = {w = 1.575, h = 2.135}}
        area = {T = {w = 14.625}, config = {view_deck = true, type = 'title'},
            cards = {card}, align_cards = function(self) self.aligned = true end,
            hard_set_cards = function(self) self.settled = true end}
        G = {TILE_W = 17.0667, UIDEF = {view_deck = function()
            return {nodes = {{config = {object = area}}}}
        end}}
    "#,
    )
    .exec()
    .unwrap();
    lua.load(include_str!("deck_layout.lua")).exec().unwrap();
    lua.load(
        r#"
        G.UIDEF.view_deck()
        assert(area.T.w < 10.4 and area.T.w > 10)
        assert(area.aligned and area.settled)
        assert(area.cards[1].T.w == 1.575 and area.cards[1].T.h == 2.135)
    "#,
    )
    .exec()
    .unwrap();
}

#[test]
fn fixed_text_keeps_its_position_when_the_font_atlas_is_resized() {
    let lua = Lua::new();
    lua.load(
        r#"
        Moveable = {extend=function() return {} end}
        G = {TILESIZE=20}
        function prep_draw() end
        love = {graphics={setColor=function() end, pop=function() end,
            draw=function(_, x, y, _, sx, sy)
                draws[#draws+1] = {x=x, y=y, sx=sx, sy=sy}
            end}}
        function sample_text(class, size, raster_scale, layout_scale)
            draws = {}
            local obj = {states={visible=true}, T={w=4, h=1},
                max_scale=0.8, gap=0.1,
                font={FONTSCALE=raster_scale, LAYOUT_FONTSCALE=layout_scale,
                    TEXT_HEIGHT_SCALE=0.83, TEXT_OFFSET={x=10, y=-20},
                    FONT={getHeight=function() return size end}},
                parts={{width=size*0.6, mult=1}, {width=size*0.3, mult=0.72}}}
            class.draw(obj)
            return draws
        end
        "#,
    )
    .exec()
    .unwrap();
    let source = crate::state::MIYOO_SMALL_SCREEN_PATCH;
    let class = &source[source.find("local FixedText =").unwrap()
        ..source.find("local function pin_dyna_scale").unwrap()];
    lua.load(format!("{class}\nFixedTextTest = FixedText"))
        .exec()
        .unwrap();
    lua.load(
        r#"
        local original = sample_text(FixedTextTest, 200, 0.1)
        local compact = sample_text(FixedTextTest, 50, 0.4, 0.1)
        for i = 1, 2 do
            assert(math.abs(original[i].x - compact[i].x) < 1e-9,
                'font atlas size changed horizontal alignment')
            assert(math.abs(original[i].y - compact[i].y) < 1e-9,
                'font atlas size changed vertical alignment')
            assert(math.abs(original[i].sy*200 - compact[i].sy*50) < 1e-9)
        end
        "#,
    )
    .exec()
    .unwrap();
}

#[test]
fn stationary_subclasses_keep_their_movement_callbacks() {
    let lua = Lua::new();
    lua.load(include_str!("movement_test_fixture.lua"))
        .exec()
        .unwrap();
    lua.load(include_str!("classified_moveables.lua"))
        .exec()
        .unwrap();
    lua.load(
        r#"
        local active, glued = {}, {}
        local count, glued_count = SVMM_collect_moveables(G.MOVEABLES, 1, false, active, glued)
        assert(count == 2 and glued_count == 1, 'subclass movement was skipped')
        assert(active[1] == G.MOVEABLES[2] and active[2] == G.MOVEABLES[4])
        assert(glued[1] == G.MOVEABLES[6])
        for i = 1, 6 do
            assert(G.MOVEABLES[i].FRAME.MOVE == (i % 2 == 1 and 1 or 0))
        end
    "#,
    )
    .exec()
    .unwrap();
}

#[test]
fn payout_labels_resize_only_inside_the_summary() {
    let lua = Lua::new();
    lua.load(
        r#"
        DynaText = {}
        UIBox = {init = function(self, args) self.config = args.config end,
                 add_child = function(self, node, parent)
            assert(parent == 'parent')
            self.node = node
            return 'added'
        end}
        G = {round_eval = setmetatable({}, {__index = UIBox})}
        Game = {update_round_eval = function(self, dt) return dt end}
        function make_text(scale, spacing)
            return setmetatable({scale=scale,
                config={pop_in=0, float=true, spacing=spacing, y_offset=3},
                font={TEXT_OFFSET={x=2, y=4}}, text_offset={},
                update_text=function(self, first)
                    assert(first and self.start_pop_in == 0)
                    assert(self.config.float and self.config.pop_in == 0)
                    self.updated = true
                end}, DynaText)
        end
        function create_UIBox_round_evaluation()
            return {config={minh=1.4}, nodes={{config={minh=30}}}}
        end
    "#,
    )
    .exec()
    .unwrap();
    lua.load(crate::state::MIYOO_PAYOUT_PATCH).exec().unwrap();
    lua.load(
        r#"
        local small, large, separator = make_text(0.36), make_text(1), make_text(0.45, 13.5)
        local node = {nodes={{config={object=small}}, {config={object=large}},
                            {config={object=separator}}}}
        local other = setmetatable({}, {__index=UIBox})
        other:add_child(node, 'parent')
        assert(small.scale == 0.36 and not small.updated)
        assert(G.round_eval:add_child(node, 'parent') == 'added')
        assert(small.scale == 0.6 and small.updated)
        assert(small.text_offset.x == 1.2 and small.text_offset.y == 5.4)
        assert(large.scale == 1 and not large.updated)
        assert(separator.scale == 0.45 and not separator.updated)
        local row = {nodes={
            {config={minw=5.5}},
            {config={minw=4.5}, nodes={{config={id='dollar_hands'}}}}
        }}
        other:add_child(row, 'parent')
        assert(row.nodes[1].config.minw == 5.5)
        G.round_eval:add_child(row, 'parent')
        assert(row.nodes[1].config.minw == 7.5 and row.nodes[2].config.minw == 2.5)
        assert(row.nodes[1].config.padding == 0.02 and row.nodes[2].config.padding == 0.02)
        local definition = create_UIBox_round_evaluation()
        assert(definition.config.minh == 1.4)
        assert(definition.nodes[1].config.minh == 0)
        assert(Game.update_round_eval({}, 0.02) == 0.02)
        assert(G.round_eval.attention_text)
        other:init({config={major={}}})
        assert(not other.attention_text)
        other:init({config={major=G.round_eval}})
        assert(other.attention_text)
    "#,
    )
    .exec()
    .unwrap();
}

#[test]
fn payout_removal_releases_its_detached_controls() {
    let lua = Lua::new();
    lua.load(
        r#"
        UIBox = {
            init = function(self, args) self.config = args.config end,
            add_child = function() end,
            remove = function(self)
                assert(not self.REMOVED, 'box removed twice')
                self.REMOVED = true
            end
        }
        Game = {update_round_eval = function() end}
        G = {}
        create_UIBox_round_evaluation = function() end
    "#,
    )
    .exec()
    .unwrap();
    lua.load(crate::state::MIYOO_PAYOUT_PATCH).exec().unwrap();
    lua.load(
        r#"
        local function box(major)
            local value = setmetatable({}, {__index=UIBox})
            value:init({config={major=major}})
            return value
        end
        local unrelated = box({})
        for i = 1, 100 do
            G.round_eval = box({})
            local payout = G.round_eval
            local cash_out, extra = box(payout), box(payout)
            assert(cash_out.attention_text and extra.attention_text)
            assert(not cash_out.parent, 'payout draw order changed')
            extra:remove()
            G.round_eval = nil
            payout:remove()
            assert(cash_out.REMOVED, 'Cash Out box survived its payout screen')
            assert(not unrelated.REMOVED, 'unrelated UI box was removed')
            assert(payout._svmm_payout_boxes == nil, 'payout retained its controls')
        end
    "#,
    )
    .exec()
    .unwrap();
}

#[test]
fn collision_preserves_edges_and_hover_buffer() {
    let lua = Lua::new();
    lua.load("Node = {}; G = {COLLISION_BUFFER = 0.1}")
        .exec()
        .unwrap();
    lua.load(SCALAR_COLLISION).exec().unwrap();
    lua.load(
        r#"
        local node = {T = {x=1, y=2, w=3, h=4, r=0}, states={hover={is=false}}}
        local hit = Node.collides_with_point
        assert(hit(node, {x=1, y=2}) == nil)
        node.container = node
        assert(hit(node, {x=1, y=2}) == true)
        assert(hit(node, {x=4, y=6}) == true)
        assert(hit(node, {x=4.01, y=6}) == nil)
        node.states.hover.is = true
        assert(hit(node, {x=4.01, y=6}) == true)
        node.CT = {x=10, y=10, w=2, h=2, r=0}
        assert(hit(node, {x=1, y=2}) == nil)
        assert(hit(node, {x=11, y=11}) == true)
        "#,
    )
    .exec()
    .unwrap();
}

#[test]
#[ignore = "requires user-owned game archive in BALATRO_TEST_GAME"]
fn collision_matches_original_game() {
    let path = std::env::var("BALATRO_TEST_GAME").expect("set BALATRO_TEST_GAME");
    let mut archive = zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
    let mut read_script = |name: &str| {
        let mut source = String::new();
        archive
            .by_name(name)
            .unwrap()
            .read_to_string(&mut source)
            .unwrap();
        source
    };
    let node = read_script("engine/node.lua");
    let helpers = read_script("functions/misc_functions.lua");
    let original = &node[node.find("function Node:collides_with_point(").unwrap()
        ..node.find("function Node:set_offset(").unwrap()];
    let helpers = &helpers[helpers.find("function point_translate(").unwrap()
        ..helpers.find("function lighten(").unwrap()];
    let lua = Lua::new();
    lua.load("Node = {}; G = {COLLISION_BUFFER = 0.1}")
        .exec()
        .unwrap();
    lua.load(helpers).exec().unwrap();
    lua.load(original).exec().unwrap();
    lua.load("original_collision = Node.collides_with_point")
        .exec()
        .unwrap();
    lua.load(SCALAR_COLLISION).exec().unwrap();
    let checked: u32 = lua
        .load(
            r#"
            local checked = 0
            local angles = {-3.14, -0.1, -0.099, 0, 0.099, 0.1, 1.57}
            local function check(node, x, y)
                local point = {x=x, y=y}
                local expected = original_collision(node, point)
                assert(Node.collides_with_point(node, point) == expected,
                    'collision differs at ' .. x .. ', ' .. y)
                assert(point.x == x and point.y == y, 'cursor was changed')
                checked = checked + 1
            end
            math.randomseed(3421)
            for _, parent_r in ipairs(angles) do
                for _, r in ipairs(angles) do
                    for kind = 1, 3 do
                        for hover = 0, 1 do
                            local T = {x=-1.25, y=2.5, w=3.75, h=4.125, r=r}
                            local node = {T=T, ARGS={}, states={hover={is=hover == 1}}}
                            if kind == 2 then node.container = node end
                            if kind == 3 then
                                node.container = {T={x=0.25, y=-0.5, w=17, h=13, r=parent_r}}
                            end
                            for use_ct = 0, 1 do
                                node.CT = use_ct == 1 and {x=2, y=-1, w=1.5, h=2, r=-r} or nil
                                local bounds = node.CT or node.T
                                for _, x in ipairs({bounds.x, bounds.x+bounds.w}) do
                                    for _, y in ipairs({bounds.y, bounds.y+bounds.h}) do
                                        for _, epsilon in ipairs({-1e-12, 0, 1e-12}) do
                                            check(node, x+epsilon, y+epsilon)
                                        end
                                    end
                                end
                                for i = 1, 200 do
                                    check(node, math.random()*20-5, math.random()*20-5)
                                end
                            end
                        end
                    end
                end
            end
            return checked
            "#,
        )
        .eval()
        .unwrap();
    assert_eq!(checked, 124_656);
}
