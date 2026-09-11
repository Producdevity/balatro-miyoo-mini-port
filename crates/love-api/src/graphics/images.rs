// Derived from balatro-port-tui (Apache-2.0). Modified by Producdevity; see NOTICE.

use super::*;

pub(super) fn register(lua: &Lua, g: &LuaTable, state: &Arc<SharedState>) -> LuaResult<()> {
    // love.graphics.newImage(path_or_imagedata [, settings]) -> Image
    {
        let s = Arc::clone(state);
        g.set(
            "newImage",
            lua.create_function(move |lua, args: LuaMultiValue| {
                // Handle newImage(ImageData) — create Image from existing ImageData
                if let Some(LuaValue::Table(t)) = args.get(0) {
                    if let Ok(img_id) = t.get::<u64>("_image_id") {
                        let images = s.images.lock();
                        if let Some(img) = images.get(&img_id) {
                            let image = Arc::clone(img);
                            drop(images);
                            return new_image_table(lua, &s, img_id, image);
                        }
                    }
                }
                let path = match args.get(0) {
                    Some(LuaValue::String(s)) => s.to_string_lossy().to_string(),
                    _ => return new_image_stub(lua, 1, 1),
                };

                if std::env::var_os("BALATRO_PLATFORM").as_deref()
                    == Some(std::ffi::OsStr::new("miyoo"))
                    && (path.ends_with("/playstack-logo.png")
                        || path.ends_with("/localthunk-logo.png"))
                {
                    return new_image_stub(lua, 1, 1);
                }

                // Load image data from game source
                let bytes = {
                    let source = s.game_source.lock();
                    source.read_file(&path).ok()
                };

                let Some(data) = bytes else {
                    eprintln!("[WARN] Image not found: '{}'", path);
                    return new_image_stub(lua, 1, 1);
                };
                let decoded = match image::load_from_memory(&data) {
                    Ok(image) => image.to_rgba8(),
                    Err(error) => {
                        eprintln!("[WARN] Failed to decode image '{}': {}", path, error);
                        return new_image_stub(lua, 1, 1);
                    }
                };
                let image = Arc::new(ImageData {
                    width: decoded.width(),
                    height: decoded.height(),
                    pixels: decoded.into_raw(),
                    white_alpha_mask: false,
                });
                let image_id = {
                    let mut next = s.next_image_id.lock();
                    let image_id = *next;
                    *next += 1;
                    image_id
                };
                s.images.lock().insert(image_id, Arc::clone(&image));
                new_image_table(lua, &s, image_id, image)
            })?,
        )?;
    }

    // love.graphics.newQuad(x, y, w, h, sw, sh) -> Quad
    // Also: newQuad(x, y, w, h, image) where image has getDimensions
    g.set(
        "newQuad",
        lua.create_function(
            |lua, (x, y, w, h, sw, sh): (f32, f32, f32, f32, LuaValue, LuaValue)| {
                let (sw_val, sh_val) = match (&sw, &sh) {
                    (LuaValue::Number(sw), LuaValue::Number(sh)) => (*sw as f32, *sh as f32),
                    (LuaValue::Integer(sw), LuaValue::Integer(sh)) => (*sw as f32, *sh as f32),
                    (LuaValue::Table(t), _) => {
                        // Image or texture passed — call getDimensions
                        let dims: Option<(u32, u32)> = t
                            .get::<LuaFunction>("getDimensions")
                            .ok()
                            .and_then(|f| f.call::<(u32, u32)>(LuaValue::Table(t.clone())).ok());
                        dims.map(|(w, h)| (w as f32, h as f32))
                            .unwrap_or((256.0, 256.0))
                    }
                    _ => (256.0, 256.0),
                };
                let quad = lua.create_table()?;
                quad.set("_is_quad", true)?;
                quad.set("_x", x)?;
                quad.set("_y", y)?;
                quad.set("_w", w)?;
                quad.set("_h", h)?;
                quad.set("_sw", sw_val)?;
                quad.set("_sh", sh_val)?;
                quad.set(
                    "_native_quad",
                    lua.create_userdata(QuadData { x, y, w, h })?,
                )?;
                quad.set(
                    "getViewport",
                    lua.create_function(|_, this: LuaTable| {
                        Ok((
                            this.get::<f32>("_x")?,
                            this.get::<f32>("_y")?,
                            this.get::<f32>("_w")?,
                            this.get::<f32>("_h")?,
                        ))
                    })?,
                )?;
                quad.set(
                    "setViewport",
                    lua.create_function(
                        |_, (this, nx, ny, nw, nh): (LuaTable, f32, f32, f32, f32)| {
                            this.set("_x", nx)?;
                            this.set("_y", ny)?;
                            this.set("_w", nw)?;
                            this.set("_h", nh)?;
                            if let Ok(handle) = this.get::<LuaAnyUserData>("_native_quad") {
                                let mut quad = handle.borrow_mut::<QuadData>()?;
                                quad.x = nx;
                                quad.y = ny;
                                quad.w = nw;
                                quad.h = nh;
                            }
                            Ok(())
                        },
                    )?,
                )?;
                quad.set(
                    "type",
                    lua.create_function(|_, _self: LuaValue| Ok("Quad"))?,
                )?;
                quad.set(
                    "typeOf",
                    lua.create_function(|_, (_self, t): (LuaValue, String)| {
                        Ok(t == "Quad" || t == "Object")
                    })?,
                )?;
                Ok(LuaValue::Table(quad))
            },
        )?,
    )?;

    // love.graphics.newMesh(vertices, mode, usage) -> Mesh
    g.set(
        "newMesh",
        lua.create_function(|lua, _args: LuaMultiValue| {
            let m = lua.create_table()?;
            m.set(
                "setVertices",
                lua.create_function(|_, _args: LuaMultiValue| Ok(()))?,
            )?;
            m.set(
                "setVertex",
                lua.create_function(|_, _args: LuaMultiValue| Ok(()))?,
            )?;
            m.set(
                "setDrawRange",
                lua.create_function(|_, _args: LuaMultiValue| Ok(()))?,
            )?;
            m.set(
                "setTexture",
                lua.create_function(|_, _args: LuaMultiValue| Ok(()))?,
            )?;
            m.set(
                "getVertexCount",
                lua.create_function(|_, _self: LuaValue| Ok(0i32))?,
            )?;
            m.set(
                "setVertexMap",
                lua.create_function(|_, _args: LuaMultiValue| Ok(()))?,
            )?;
            m.set(
                "attachAttribute",
                lua.create_function(|_, _args: LuaMultiValue| Ok(()))?,
            )?;
            m.set("release", lua.create_function(|_, _self: LuaValue| Ok(()))?)?;
            m.set(
                "type",
                lua.create_function(|_, _self: LuaValue| Ok("Mesh"))?,
            )?;
            m.set(
                "typeOf",
                lua.create_function(|_, (_self, t): (LuaValue, String)| {
                    Ok(t == "Mesh" || t == "Drawable" || t == "Object")
                })?,
            )?;
            Ok(LuaValue::Table(m))
        })?,
    )?;

    Ok(())
}

/// Create an ImageData table (love.image.newImageData compatible)
pub(super) fn create_image_data_table(
    lua: &Lua,
    state: &Arc<SharedState>,
    w: u32,
    h: u32,
) -> LuaResult<LuaValue> {
    // Create pixel data (RGBA, initialized to transparent black)
    let pixels = vec![0u8; (w * h * 4) as usize];
    let id = {
        let mut next = state.next_image_id.lock();
        let id = *next;
        *next += 1;
        id
    };
    state.images.lock().insert(
        id,
        Arc::new(ImageData {
            width: w,
            height: h,
            pixels,
            white_alpha_mask: false,
        }),
    );

    let idata = lua.create_table()?;
    idata.set("_image_id", id)?;
    idata.set(
        "getWidth",
        lua.create_function(move |_, _self: LuaValue| Ok(w))?,
    )?;
    idata.set(
        "getHeight",
        lua.create_function(move |_, _self: LuaValue| Ok(h))?,
    )?;
    idata.set(
        "getDimensions",
        lua.create_function(move |_, _self: LuaValue| Ok((w, h)))?,
    )?;
    idata.set(
        "getPixel",
        lua.create_function(move |_, (_self, _x, _y): (LuaValue, u32, u32)| {
            Ok((0.0f64, 0.0f64, 0.0f64, 0.0f64))
        })?,
    )?;
    idata.set(
        "setPixel",
        lua.create_function(|_, _args: LuaMultiValue| Ok(()))?,
    )?;
    idata.set(
        "paste",
        lua.create_function(|_, _args: LuaMultiValue| Ok(()))?,
    )?;
    idata.set("release", lua.create_function(|_, _self: LuaValue| Ok(()))?)?;
    idata.set(
        "type",
        lua.create_function(|_, _self: LuaValue| Ok("ImageData"))?,
    )?;
    idata.set(
        "typeOf",
        lua.create_function(|_, (_self, t): (LuaValue, String)| {
            Ok(t == "ImageData" || t == "Data" || t == "Object")
        })?,
    )?;
    Ok(LuaValue::Table(idata))
}

pub(super) fn new_image_table(
    lua: &Lua,
    state: &Arc<SharedState>,
    image_id: u64,
    image: Arc<ImageData>,
) -> LuaResult<LuaValue> {
    let w = image.width;
    let h = image.height;
    let img = lua.create_table()?;
    img.set("_image_id", image_id)?;
    img.set(
        "_native_image",
        lua.create_userdata(ImageHandle {
            id: image_id,
            data: image,
        })?,
    )?;
    img.set(
        "getDimensions",
        lua.create_function(move |_, _self: LuaValue| Ok((w, h)))?,
    )?;
    img.set(
        "getWidth",
        lua.create_function(move |_, _self: LuaValue| Ok(w))?,
    )?;
    img.set(
        "getHeight",
        lua.create_function(move |_, _self: LuaValue| Ok(h))?,
    )?;
    img.set(
        "getPixelWidth",
        lua.create_function(move |_, _self: LuaValue| Ok(w))?,
    )?;
    img.set(
        "getPixelHeight",
        lua.create_function(move |_, _self: LuaValue| Ok(h))?,
    )?;
    img.set(
        "setFilter",
        lua.create_function(|_, _args: LuaMultiValue| Ok(()))?,
    )?;
    img.set(
        "getFilter",
        lua.create_function(|_, _self: LuaValue| Ok(("nearest", "nearest")))?,
    )?;
    img.set(
        "setWrap",
        lua.create_function(|_, _args: LuaMultiValue| Ok(()))?,
    )?;
    img.set(
        "getWrap",
        lua.create_function(|_, _self: LuaValue| Ok(("clamp", "clamp")))?,
    )?;
    {
        let sr = Arc::clone(state);
        img.set(
            "release",
            lua.create_function(move |_, self_tbl: LuaTable| {
                if let Ok(id) = self_tbl.get::<u64>("_image_id") {
                    sr.images.lock().remove(&id);
                }
                // Remove the GC guard so it doesn't double-free
                self_tbl.set("_gc_guard", LuaValue::Nil).ok();
                self_tbl.set("_native_image", LuaValue::Nil).ok();
                Ok(())
            })?,
        )?;
    }
    img.set(
        "type",
        lua.create_function(|_, _self: LuaValue| Ok("Image"))?,
    )?;
    img.set(
        "typeOf",
        lua.create_function(|_, (_self, t): (LuaValue, String)| {
            Ok(t == "Image" || t == "Texture" || t == "Drawable" || t == "Object")
        })?,
    )?;

    // Attach GC guard — when Lua GC collects this table, the userdata's Drop frees the image
    let guard = lua.create_userdata(ResourceGuard {
        id: image_id,
        kind: ResourceKind::Image,
        state: Arc::clone(state),
    })?;
    img.set("_gc_guard", guard)?;

    Ok(LuaValue::Table(img))
}

pub(super) fn new_image_stub(lua: &Lua, w: u32, h: u32) -> LuaResult<LuaValue> {
    let img = lua.create_table()?;
    img.set(
        "getDimensions",
        lua.create_function(move |_, _self: LuaValue| Ok((w, h)))?,
    )?;
    img.set(
        "getWidth",
        lua.create_function(move |_, _self: LuaValue| Ok(w))?,
    )?;
    img.set(
        "getHeight",
        lua.create_function(move |_, _self: LuaValue| Ok(h))?,
    )?;
    img.set(
        "getPixelWidth",
        lua.create_function(move |_, _self: LuaValue| Ok(w))?,
    )?;
    img.set(
        "getPixelHeight",
        lua.create_function(move |_, _self: LuaValue| Ok(h))?,
    )?;
    img.set(
        "setFilter",
        lua.create_function(|_, _args: LuaMultiValue| Ok(()))?,
    )?;
    img.set(
        "getFilter",
        lua.create_function(|_, _self: LuaValue| Ok(("nearest", "nearest")))?,
    )?;
    img.set(
        "setWrap",
        lua.create_function(|_, _args: LuaMultiValue| Ok(()))?,
    )?;
    img.set(
        "getWrap",
        lua.create_function(|_, _self: LuaValue| Ok(("clamp", "clamp")))?,
    )?;
    img.set("release", lua.create_function(|_, _self: LuaValue| Ok(()))?)?;
    img.set(
        "type",
        lua.create_function(|_, _self: LuaValue| Ok("Image"))?,
    )?;
    img.set(
        "typeOf",
        lua.create_function(|_, (_self, t): (LuaValue, String)| {
            Ok(t == "Image" || t == "Texture" || t == "Drawable" || t == "Object")
        })?,
    )?;
    Ok(LuaValue::Table(img))
}
