use super::*;

fn check_draw(transform: Transform, angle: f32, scale: (f32, f32), clipped: bool) {
    let mut source = Vec::new();
    for y in 0..10u8 {
        for x in 0..12u8 {
            source.extend_from_slice(&[x * 19, y * 23, 97, 255]);
        }
    }
    let mut actual = PixelBuffer::new(64, 48);
    let mut expected = PixelBuffer::new(64, 48);
    if clipped {
        actual.scissor = Some((20, 15, 24, 19));
        expected.scissor = actual.scissor;
    }

    let mut composite = transform.clone();
    composite.translate(3.0, 2.0);
    composite.rotate(angle);
    composite.scale(scale.0, scale.1);
    composite.translate(-2.0, -1.0);
    let inv = composite.inverse().unwrap();
    expected.draw_image_region_transformed(
        &source,
        12,
        2.0,
        1.0,
        7.0,
        6.0,
        (0, 0, 64, 48),
        [inv.a, inv.b, inv.tx, inv.c, inv.d, inv.ty],
        [255; 4],
        false,
        false,
        DissolveParams::NONE,
    );
    draw_region_to_buf(
        &mut actual,
        &source,
        12,
        10,
        2.0,
        1.0,
        7.0,
        6.0,
        3.0,
        2.0,
        angle,
        scale.0,
        scale.1,
        2.0,
        1.0,
        &transform,
        [255; 4],
        false,
        false,
        DissolveParams::NONE,
        None,
    );

    assert!(expected.pixels.iter().any(|&v| v != 0));
    let mismatches = actual
        .pixels
        .iter()
        .zip(&expected.pixels)
        .filter(|(a, b)| a != b)
        .count();
    assert_eq!(
        mismatches, 0,
        "transform={transform:?} angle={angle} scale={scale:?} clipped={clipped}"
    );
}

#[test]
fn image_draw_honors_the_transform_stack() {
    for angle in [-0.35, 0.0005, 0.22, std::f32::consts::FRAC_PI_2] {
        let mut transform = Transform::default();
        transform.translate(32.0, 20.0);
        transform.rotate(angle);
        for clipped in [false, true] {
            check_draw(transform.clone(), 0.0, (2.0, 2.0), clipped);
            check_draw(transform.clone(), 0.17, (1.5, 2.5), clipped);
        }
    }
}

#[test]
fn image_draw_honors_shear_and_reflection() {
    for (a, b, c, d) in [
        (-1.0, 0.0, 0.0, 1.0),
        (1.0, 0.0, 0.0, -1.0),
        (1.0, 0.3, -0.2, 1.0),
    ] {
        let transform = Transform {
            a,
            b,
            c,
            d,
            tx: 32.0,
            ty: 20.0,
        };
        for clipped in [false, true] {
            check_draw(transform.clone(), 0.0, (2.0, 2.0), clipped);
        }
    }
}

#[test]
fn axis_aligned_draws_keep_the_fast_path_output() {
    let mut transform = Transform::default();
    transform.translate(20.0, 14.0);
    check_draw(transform.clone(), 0.0, (2.0, 2.0), false);
    check_draw(transform, 0.0, (-2.0, 2.0), true);
}
