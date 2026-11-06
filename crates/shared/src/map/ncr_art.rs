//! Human-scale urban kit. Visual detail is separate from the tested movement geometry.
use super::{
    Mat, Shape,
    build::{Builder, Frame},
};
use bevy::prelude::*;
use std::f32::consts::FRAC_PI_2;

pub fn tower(b: &mut Builder, f: Frame, w: f32, d: f32, h: f32, glass: bool) {
    b.deco(
        f,
        Vec3::new(0.0, 0.015, 0.0),
        Vec3::new(w + 12.0, 0.03, d + 12.0),
        Mat::Pavers,
    );
    // A distinct ground floor and a recessed glazing band establish the human scale.
    for z in [-d / 2.0 - 0.12, d / 2.0 + 0.12] {
        b.deco(
            f,
            Vec3::new(0.0, 1.7, z),
            Vec3::new(w - 1.0, 3.0, 0.12),
            Mat::Glass,
        );
        b.deco(
            f,
            Vec3::new(0.0, 3.4, z),
            Vec3::new(w + 0.5, 0.28, 0.7),
            Mat::Concrete,
        );
        for i in 0..(w / 3.0) as usize {
            let x = -w / 2.0 + 1.5 + i as f32 * 3.0;
            b.deco(
                f,
                Vec3::new(x, 1.7, z + z.signum() * 0.12),
                Vec3::new(0.09, 3.1, 0.1),
                Mat::Steel,
            );
        }
    }
    if !glass {
        // Painted piers split the strip windows; AC boxes and drip shades break perfect repetition.
        for floor in 1..(h / 3.8) as usize {
            for i in 0..(w / 4.0) as usize {
                let x = -w / 2.0 + 1.5 + i as f32 * 4.0;
                let y = floor as f32 * 3.8;
                for z in [-d / 2.0 - 0.15, d / 2.0 + 0.15] {
                    b.deco(
                        f,
                        Vec3::new(x, y, z),
                        Vec3::new(0.55, 1.5, 0.18),
                        Mat::Plaster,
                    );
                    b.deco(
                        f,
                        Vec3::new(x + 1.3, y + 0.9, z),
                        Vec3::new(2.3, 0.12, 0.7),
                        Mat::Concrete,
                    );
                    if (i + floor) % 3 == 0 {
                        b.deco(
                            f,
                            Vec3::new(x + 0.9, y - 0.1, z + z.signum() * 0.28),
                            Vec3::new(0.75, 0.5, 0.4),
                            Mat::PaintWhite,
                        );
                        for k in 0..4 {
                            b.deco(
                                f,
                                Vec3::new(x + 0.9, y - 0.27 + k as f32 * 0.1, z + z.signum() * 0.5),
                                Vec3::new(0.58, 0.025, 0.02),
                                Mat::Steel,
                            );
                        }
                    }
                }
            }
        }
    } else {
        for z in [-d / 2.0 - 0.3, d / 2.0 + 0.3] {
            for x in [-w / 2.0, w / 2.0] {
                b.deco(
                    f,
                    Vec3::new(x, h / 2.0, z),
                    Vec3::new(0.5, h, 0.4),
                    Mat::Concrete,
                );
            }
        }
    }
    // Roof mechanical enclosure, louvres, access door and compact aerial.
    let c = Vec3::new(w * 0.25, h + 0.85, d * 0.2);
    for i in 0..8 {
        b.deco(
            f,
            c + Vec3::new(-1.3 + i as f32 * 0.37, 0.7, 0.0),
            Vec3::new(0.12, 0.08, 1.8),
            Mat::Rubber,
        );
    }
    b.deco(
        f,
        Vec3::new(-w * 0.32, h + 2.5, d * 0.3),
        Vec3::new(0.08, 5.0, 0.08),
        Mat::Steel,
    );
    b.deco(
        f,
        Vec3::new(-w * 0.32, h + 4.0, d * 0.3),
        Vec3::new(1.5, 0.06, 0.06),
        Mat::Steel,
    );
}

pub fn pavilion(b: &mut Builder, f: Frame, w: f32, d: f32, h: f32, mat: Mat) {
    b.deco(
        f,
        Vec3::new(0.0, 0.015, 0.0),
        Vec3::new(w + 5.0, 0.03, d + 6.0),
        Mat::Pavers,
    );
    for z in [-d / 2.0 - 0.12, d / 2.0 + 0.12] {
        // Cornice, framed fascia and fine column capitals instead of bare slab edges.
        b.deco(
            f,
            Vec3::new(0.0, h - 0.35, z),
            Vec3::new(w + 0.5, 0.45, 0.45),
            Mat::Plaster,
        );
        b.deco(
            f,
            Vec3::new(0.0, h + 0.12, z),
            Vec3::new(w + 0.7, 0.15, 0.6),
            Mat::Concrete,
        );
        b.deco(
            f,
            Vec3::new(0.0, 3.55, z + z.signum() * 0.25),
            Vec3::new(7.5, 1.5, 0.12),
            Mat::ShopSign,
        );
        for x in [-w * 0.3, w * 0.3] {
            b.deco(
                f,
                Vec3::new(x, 0.3, z),
                Vec3::new(0.9, 0.6, 0.9),
                Mat::Concrete,
            );
            b.deco(
                f,
                Vec3::new(x, h - 0.65, z),
                Vec3::new(1.0, 0.3, 0.85),
                Mat::Plaster,
            );
        }
    }
    for x in [-w / 2.0 + 0.65, w / 2.0 - 0.65] {
        // Display cabinets sit against solid walls, leaving the centre lane open.
        b.deco(
            f,
            Vec3::new(x, 1.1, 0.0),
            Vec3::new(0.4, 2.2, d * 0.55),
            Mat::Steel,
        );
        b.deco(
            f,
            Vec3::new(x - x.signum() * 0.23, 1.25, 0.0),
            Vec3::new(0.04, 1.65, d * 0.5),
            Mat::Glass,
        );
        for z in [-d * 0.18, d * 0.18] {
            b.deco(
                f,
                Vec3::new(x - x.signum() * 0.28, 1.3, z),
                Vec3::new(0.06, 1.9, 0.08),
                Mat::PaintWhite,
            );
        }
    }
    if mat == Mat::Plaster {
        // Warm lit interior panels; no new shadow-casting dynamic lights.
        for x in [-w * 0.25, w * 0.25] {
            b.deco(
                f,
                Vec3::new(x, h - 0.25, 0.0),
                Vec3::new(1.5, 0.08, 0.45),
                Mat::TransitYellow,
            );
        }
    }
}

fn car(b: &mut Builder, f: Frame, paint: Mat) {
    // Static prop: a single simple collision hull, dressed with body panels and glass.
    b.push(
        f.p(Vec3::new(0.0, 0.75, 0.0)),
        f.rot(),
        Shape::Box(Vec3::new(1.85, 1.45, 4.3)),
        paint,
        true,
        false,
    );
    b.deco(
        f,
        Vec3::new(0.0, 0.68, 0.0),
        Vec3::new(1.85, 0.65, 4.3),
        paint,
    );
    b.deco(
        f,
        Vec3::new(0.0, 1.15, -0.2),
        Vec3::new(1.65, 0.72, 2.35),
        Mat::Glass,
    );
    b.deco(
        f,
        Vec3::new(0.0, 1.55, -0.2),
        Vec3::new(1.68, 0.12, 2.2),
        paint,
    );
    for x in [-0.86, 0.86] {
        for z in [-1.35, 1.35] {
            let rot = f.rot() * Quat::from_rotation_z(FRAC_PI_2);
            b.push(
                f.p(Vec3::new(x, 0.42, z)),
                rot,
                Shape::Cylinder {
                    radius: 0.38,
                    height: 0.26,
                },
                Mat::Rubber,
                false,
                true,
            );
            b.push(
                f.p(Vec3::new(x + x.signum() * 0.15, 0.42, z)),
                rot,
                Shape::Cylinder {
                    radius: 0.22,
                    height: 0.025,
                },
                Mat::Steel,
                false,
                true,
            );
        }

        b.deco(
            f,
            Vec3::new(x, 1.2, -0.1),
            Vec3::new(0.08, 0.75, 0.12),
            paint,
        );
        for z in [-1.3, 0.9] {
            b.deco(f, Vec3::new(x, 1.2, z), Vec3::new(0.1, 0.7, 0.13), paint);
        }
        b.deco(
            f,
            Vec3::new(x * 1.1, 1.1, -1.1),
            Vec3::new(0.28, 0.18, 0.3),
            paint,
        );
    }
    for z in [-2.17, 2.17] {
        b.deco(
            f,
            Vec3::new(0.0, 0.6, z),
            Vec3::new(1.8, 0.18, 0.12),
            Mat::Rubber,
        );
        for x in [-0.65, 0.65] {
            b.deco(
                f,
                Vec3::new(x, 0.85, z),
                Vec3::new(0.45, 0.16, 0.05),
                if z < 0.0 {
                    Mat::PaintWhite
                } else {
                    Mat::ContainerRed
                },
            );
        }
        b.deco(
            f,
            Vec3::new(0.0, 0.56, z + z.signum() * 0.07),
            Vec3::new(0.45, 0.12, 0.02),
            Mat::PaintWhite,
        );
    }
}

fn bus(b: &mut Builder, f: Frame, paint: Mat) {
    b.push(
        f.p(Vec3::new(0.0, 1.15, 0.0)),
        f.rot(),
        Shape::Box(Vec3::new(2.6, 2.3, 10.5)),
        paint,
        true,
        false,
    );
    b.deco(f, Vec3::new(0.0, 1.8, -0.15), Vec3::new(2.35, 1.05, 8.3), Mat::Glass);
    for z in [-4.3, -2.2, 0.0, 2.2, 4.3] {
        for x in [-1.23, 1.23] {
            b.deco(f, Vec3::new(x, 1.45, z), Vec3::new(0.08, 0.85, 1.35), Mat::Steel);
        }
    }
    for x in [-1.18, 1.18] {
        for z in [-3.6, 3.6] {
            b.push(
                f.p(Vec3::new(x, 0.55, z)),
                f.rot() * Quat::from_rotation_z(FRAC_PI_2),
                Shape::Cylinder { radius: 0.5, height: 0.3 },
                Mat::Rubber,
                false,
                true,
            );
        }
    }
    for z in [-5.25, 5.25] {
        b.deco(f, Vec3::new(0.0, 0.9, z), Vec3::new(2.4, 0.22, 0.12), Mat::PaintWhite);
    }
}

fn bench(b: &mut Builder, f: Frame) {
    for z in [-0.25, 0.0, 0.25] {
        b.deco(
            f,
            Vec3::new(0.0, 0.5, z),
            Vec3::new(1.9, 0.08, 0.18),
            Mat::Wood,
        );
    }
    for y in [0.85, 1.08] {
        b.deco(
            f,
            Vec3::new(0.0, y, 0.3),
            Vec3::new(1.9, 0.17, 0.09),
            Mat::Wood,
        );
    }
    for x in [-0.7, 0.7] {
        b.deco(
            f,
            Vec3::new(x, 0.5, 0.25),
            Vec3::new(0.08, 1.0, 0.08),
            Mat::Steel,
        );
    }
}
pub fn streets(b: &mut Builder) {
    backdrop(b);
    let f = Frame::new(Vec3::ZERO, 0.0);
    // Parking sits in building forecourts, outside the arterial travel lanes.
    for (x, z, yaw) in [
        (-365.0, 110.0, 0.0),
        (-365.0, 117.0, 0.0),
        (-302.0, 175.0, 0.0),
        (-239.0, 240.0, 0.0),
        (-176.0, 105.0, 0.0),
        (-270.0, 326.0, 0.0),
        (150.0, 115.0, 1.57),
        (230.0, 195.0, 1.57),
        (390.0, 275.0, 1.57),
        (-340.0, -350.0, 1.57),
        (-300.0, -350.0, 1.57),
    ] {
        car(
            b,
            Frame::new(Vec3::new(x, 0.0, z), yaw),
            if x < -300.0 {
                Mat::PaintWhite
            } else {
                Mat::ContainerRed
            },
        );
    }
    for (x, z, yaw) in [(-285.0, 70.0, 0.0), (205.0, 175.0, FRAC_PI_2), (330.0, -235.0, 0.0)] {
        bus(
            b,
            Frame::new(Vec3::new(x, 0.0, z), yaw),
            if x < 0.0 { Mat::TransitBlue } else { Mat::TransitYellow },
        );
    }
    for (x, z) in [
        (-307.0, 260.0),
        (-244.0, 260.0),
        (-181.0, 260.0),
        (170.0, 130.0),
        (250.0, 210.0),
    ] {
        for k in 0..7 {
            b.deco(
                f,
                Vec3::new(x + k as f32 * 0.9, 0.14, z),
                Vec3::new(0.45, 0.02, 10.0),
                Mat::PaintWhite,
            );
        }
    }
    for (x, z) in [
        (-304.0, 248.0),
        (-241.0, 248.0),
        (-178.0, 248.0),
        (-225.0, 360.0),
        (150.0, 125.0),
        (230.0, 205.0),
        (-230.0, -272.0),
    ] {
        bench(b, Frame::new(Vec3::new(x, 0.0, z), 0.0));
        b.deco_cyl(f, Vec3::new(x + 2.0, 0.0, z), 0.3, 0.9, Mat::Steel);
        b.deco_cyl(f, Vec3::new(x + 2.0, 0.88, z), 0.32, 0.1, Mat::Rubber);
    }
    // Transit entrance signs and bollards stand clear of stair centre lines.
    for x in [-175.0, 175.0] {
        b.deco(
            f,
            Vec3::new(x, 3.2, 7.0),
            Vec3::new(5.0, 1.25, 0.15),
            Mat::MetroSign,
        );
        for dx in [-2.4, 2.4] {
            b.deco(
                f,
                Vec3::new(x + dx, 1.6, 7.0),
                Vec3::new(0.1, 3.2, 0.1),
                Mat::Steel,
            );
        }
    }
    // Service cables and AC service runs in the old-city lanes.
    for z in [-424.0, -395.0] {
        for i in 0..20 {
            let x = -370.0 + i as f32 * 11.0;
            let y = 6.0 - 0.7 * (std::f32::consts::PI * i as f32 / 20.0).sin();
            b.deco(
                f,
                Vec3::new(x, y, z),
                Vec3::new(11.1, 0.035, 0.035),
                Mat::Rubber,
            );
        }
    }
    // Freight stacks get door frames, locking bars and ribs on both long faces.
    for ix in 0..12 {
        for iz in 0..14 {
            if iz == 4 || iz == 9 || ix == 5 {
                continue;
            }
            let (x, z) = (215.0 + ix as f32 * 20.0, -295.0 + iz as f32 * 16.0);
            let h = if (ix + iz) % 3 == 0 { 5.4 } else { 2.7 };
            for zz in [-2.54, 2.54] {
                for k in 0..15 {
                    b.deco(
                        f,
                        Vec3::new(x - 5.7 + k as f32 * 0.8, h / 2.0, z + zz),
                        Vec3::new(0.08, h - 0.14, 0.06),
                        if ix % 2 == 0 {
                            Mat::TransitBlue
                        } else {
                            Mat::ContainerRed
                        },
                    );
                }
            }
            for zz in [-1.2, 1.2] {
                b.deco(
                    f,
                    Vec3::new(x + 6.06, h / 2.0, z + zz),
                    Vec3::new(0.08, h - 0.2, 0.08),
                    Mat::Steel,
                );
            }
            b.deco(
                f,
                Vec3::new(x + 6.08, 1.0, z),
                Vec3::new(0.05, 0.25, 0.8),
                Mat::PaintWhite,
            );
        }
    }
}

/// A low-cost, non-playable city silhouette removes the floating-board horizon.
fn backdrop(b: &mut Builder) {
    let f = Frame::new(Vec3::ZERO, 0.0);
    b.deco(
        f,
        Vec3::new(0.0, -4.2, 0.0),
        Vec3::new(2600.0, 0.1, 2600.0),
        Mat::Dirt,
    );
    for i in 0..16 {
        let x = -410.0 + i as f32 * 64.0;
        if x.abs() < 65.0 {
            continue;
        }
        for z in [-650.0, 650.0] {
            let h = 22.0 + ((i * 17) % 41) as f32;
            let f = Frame::new(Vec3::new(x, -4.0, z), 0.0);
            b.deco(
                f,
                Vec3::new(0.0, h / 2.0, 0.0),
                Vec3::new(33.0, h, 38.0),
                if i % 3 == 0 {
                    Mat::Glass
                } else {
                    Mat::Concrete
                },
            );
            for floor in 1..(h / 4.0) as usize {
                for z in [-19.1, 19.1] {
                    b.deco(
                        f,
                        Vec3::new(0.0, floor as f32 * 4.0, z),
                        Vec3::new(31.0, 1.2, 0.08),
                        Mat::Glass,
                    );
                }
            }
        }
    }
    for i in 0..15 {
        let h = 18.0 + ((i * 13) % 35) as f32;
        b.deco(
            f,
            Vec3::new(660.0, h / 2.0 - 4.0, -500.0 + i as f32 * 75.0),
            Vec3::new(45.0, h, 38.0),
            Mat::Concrete,
        );
    }
}
