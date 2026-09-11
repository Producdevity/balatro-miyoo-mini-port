use mlua::Lua;
use std::io::Read;

#[test]
#[ignore = "requires user-owned game archive in BALATRO_TEST_GAME"]
fn menu_events_keep_their_duration_at_low_frame_rates() {
    let path = std::env::var("BALATRO_TEST_GAME").expect("set BALATRO_TEST_GAME");
    let mut archive = zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
    let lua = Lua::new();
    for name in ["engine/object.lua", "engine/event.lua"] {
        let mut source = String::new();
        archive
            .by_name(name)
            .unwrap()
            .read_to_string(&mut source)
            .unwrap();
        lua.load(&source).exec().unwrap();
    }
    lua.load(
        r#"
        now = 0
        love = {timer={getTime=function() return now end}}
        function reset()
            now = 0
            G = {SETTINGS={paused=true}, TIMERS={REAL=0, TOTAL=0}, ARGS={}}
            manager = EventManager()
        end
        function reveal_duration()
            reset()
            local card = {dissolve=1}
            manager:add_event(Event{trigger='ease', ref_table=card,
                ref_value='dissolve', ease_to=0, delay=0.6})
            manager:update(0, true)
            while card.dissolve > 0 do
                now = now + 0.2
                G.TIMERS.REAL = G.TIMERS.REAL + 0.1
                manager:update(0.1)
                assert(now < 2)
            end
            return now
        end
        original_duration = reveal_duration()
    "#,
    )
    .exec()
    .unwrap();
    lua.load(include_str!("menu_clock.lua")).exec().unwrap();
    lua.load(
        r#"
        assert(original_duration >= 1.2, 'test did not reproduce capped-clock slowdown')
        assert(reveal_duration() <= 0.8, 'menu reveal still follows the capped clock')
        reset()
        local explicit = Event{timer='REAL', delay=0.6}
        assert(explicit.timer == 'REAL', 'explicit clock was overwritten')
        G.SETTINGS.paused = false
        local gameplay = Event{trigger='after', delay=0.6}
        assert(gameplay.timer == 'TOTAL', 'gameplay clock was changed')
        manager:add_event(gameplay)
        manager:update(0, true)
        G.SETTINGS.paused = true
        now = 600
        manager:update(0.1)
        assert(not gameplay.complete, 'paused gameplay advanced during sleep')
        G.SETTINGS.paused = false
        G.TIMERS.TOTAL = 0.61
        manager:update(0.1)
        assert(gameplay.complete)
        reset()
        local menu = Event{trigger='after', delay=0.6}
        manager:add_event(menu)
        manager:update(0, true)
        now = 600
        manager:update(0.1)
        assert(menu.complete, 'menu transition stayed stuck after a long pause')
    "#,
    )
    .exec()
    .unwrap();
}
