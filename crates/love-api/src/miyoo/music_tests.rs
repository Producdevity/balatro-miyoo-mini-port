use mlua::Lua;
use std::io::Read;

fn original_sound_script() -> String {
    let path = std::env::var("BALATRO_TEST_GAME").expect("set BALATRO_TEST_GAME");
    let mut archive = zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
    let mut source = String::new();
    archive
        .by_name("functions/misc_functions.lua")
        .unwrap()
        .read_to_string(&mut source)
        .unwrap();
    source
}

#[test]
#[ignore = "requires user-owned game archive in BALATRO_TEST_GAME"]
fn music_crossfades_follow_elapsed_time_without_changing_effects_or_pitch() {
    let source = original_sound_script();
    let lua = Lua::new();
    lua.load(&source).exec().unwrap();
    lua.load(
        r#"
        now = 0
        love = {timer={getTime=function() return now end}}
        args = {dt=0.1, desired_track='music4', pitch_mod=1,
            sound_settings={volume=100, music_volume=100, game_sounds_volume=80}}
        function track(code, volume)
            return {sound_code=code, current_volume=volume,
                original_volume=0.6, original_pitch=0.7,
                sound={setVolume=function(s,v) s.volume=v end,
                       setPitch=function(s,v) s.pitch=v end,
                       stop=function() error('unexpected stop') end}}
        end
        function fade(step, game_dt)
            now = 0
            args.dt = 0
            local old, new = track('music1',1), track('music4',0)
            SET_SFX(old,args); SET_SFX(new,args)
            args.dt = game_dt
            for i=1,math.floor(1/step + 0.5) do
                now=i*step
                SET_SFX(old,args); SET_SFX(new,args)
                assert(args.dt == game_dt, 'game frame delta was overwritten')
            end
            assert(old.sound.pitch == 0.7 and new.sound.pitch == 0.7)
            return old.sound.volume, new.sound.volume
        end
        original_slow = fade(0.2,0.1)
        effect = track('chips1',1)
        SET_SFX(effect,args)
        effect_volume, effect_pitch = effect.sound.volume, effect.sound.pitch
    "#,
    )
    .exec()
    .unwrap();
    let mut patched = source.into_bytes();
    super::patch_miyoo_script("functions/misc_functions.lua", &mut patched);
    lua.load(&patched).exec().unwrap();
    lua.load(
        r#"
        local slow, slow_in = fade(0.2,0.1)
        local fast, fast_in = fade(1/60,1/60)
        assert(original_slow > slow*3, 'original slow-frame overlap was not reproduced')
        assert(math.abs(slow-fast) < 1e-12 and math.abs(slow_in-fast_in) < 1e-12)
        assert(math.abs(slow + slow_in - 0.6) < 1e-12, 'crossfade gain changed')
        SET_SFX(effect,args)
        assert(effect.sound.volume == effect_volume and effect.sound.pitch == effect_pitch)
        args.dt = 0.1
        local old = track('music1',1)
        SET_SFX(old,args)
        now = now + 600
        SET_SFX(old,args)
        assert(old.sound.volume == 0, 'old track survived a long sleep')
        args.desired_track = 'music1'
        now = now + 0.1
        SET_SFX(old,args)
        assert(old.sound.volume > 0 and old.sound.volume < 0.6)
    "#,
    )
    .exec()
    .unwrap();
}

#[test]
#[ignore = "requires user-owned game archive in BALATRO_TEST_GAME"]
fn music_restarts_prepare_every_layer_before_grouped_playback() {
    let mut source = original_sound_script().into_bytes();
    super::patch_miyoo_script("functions/misc_functions.lua", &mut source);
    let lua = Lua::new();
    lua.load(&source).exec().unwrap();
    lua.load(
        r#"
        local created, groups, single = 0, 0, 0
        love = {timer={getTime=function() return 0 end}, audio={}}
        love.audio.newSource = function()
            created = created + 1
            return {
                setVolume=function(s,v) s.volume=v end,
                setPitch=function(s,v) s.pitch=v end,
                stop=function(s) s.playing=false end,
                isPlaying=function(s) return s.playing end,
            }
        end
        love.audio.play = function(sources)
            if sources[1] then
                groups = groups + 1
                assert(created == groups*5, 'music played before all layers were prepared')
                assert(#sources == 5)
                for _, s in ipairs(sources) do
                    assert(s.pitch == 0.7)
                    s.playing = true
                end
            else
                single = single + 1
                sources.playing = true
            end
        end
        SOURCES = {music1={},music2={},music3={},music4={},music5={}}
        local args = {desired_track='music1', dt=0.016, pitch_mod=1,
            sound_settings={volume=100,music_volume=100,game_sounds_volume=100}}
        RESTART_MUSIC(args)
        local old = {}
        for _, sources in pairs(SOURCES) do old[#old+1] = sources[1].sound end
        RESTART_MUSIC(args)
        for _, sound in ipairs(old) do assert(not sound.playing, 'old music kept playing') end
        assert(groups == 2 and single == 0)
        for _, sources in pairs(SOURCES) do
            assert(#sources == 1 and sources[1].sound.playing and sources[1].initialized)
        end
        args.sound_code = 'chips1'
        PLAY_SOUND(args)
        assert(single == 1, 'sound effect was delayed with music')
    "#,
    )
    .exec()
    .unwrap();
}
