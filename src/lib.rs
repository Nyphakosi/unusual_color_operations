use std::io;

use image::Rgb;

// rgb↔hsv conversion functions taken from https://gist.github.com/bmgxyz/a5b5b58e492cbca099b468eddd04cc97

pub struct Hsv(pub [f32; 3]);

pub fn rgb_to_hsv(pixel: &Rgb<u8>) -> Hsv {
    let [r, g, b] = pixel.0;
    let big_m = *[r, g, b].iter().max().unwrap() as f32 / 255.;
    let little_m = *[r, g, b].iter().min().unwrap() as f32 / 255.;
    let c = big_m - little_m;
    let s = (c / big_m) * 100.;
    let (little_r, little_g, little_b) = (r as f32 / 255., g as f32 / 255., b as f32 / 255.);
    let (big_r, big_g, big_b) = (
        (big_m - little_r) / c,
        (big_m - little_g) / c,
        (big_m - little_b) / c,
    );
    let h_prime = match big_m {
        x if x == little_m => 0.,
        x if x == little_r => big_b - big_g,
        x if x == little_g => 2. + big_r - big_b,
        x if x == little_b => 4. + big_g - big_r,
        _ => unreachable!(),
    };
    let h = h_prime / 6. * 360.;
    let v = big_m * 100.;
    Hsv([h, s, v])
}

pub fn hsv_to_rgb(pixel: &Hsv) -> Rgb<u8> {
    let [hue, saturation, value] = [pixel.0[0], pixel.0[1] / 100., pixel.0[2] / 100.];
    let max = value;
    let c = saturation * value;
    let min = max - c;
    let h_prime = if hue >= 300. {
        (hue - 360.) / 60.
    } else {
        hue / 60.
    };
    let (r, g, b) = match h_prime {
        x if (-1. ..1.).contains(&x) => {
            if h_prime < 0. {
                (max, min, min - h_prime * c)
            } else {
                (max, min + h_prime * c, min)
            }
        }
        x if (1. ..3.).contains(&x) => {
            if h_prime < 2. {
                (min - (h_prime - 2.) * c, max, min)
            } else {
                (min, max, min + (h_prime - 2.) * c)
            }
        }
        x if (3. ..5.).contains(&x) => {
            if h_prime < 4. {
                (min, min - (h_prime - 4.) * c, max)
            } else {
                (min + (h_prime - 4.) * c, min, max)
            }
        }
        _ => unreachable!(),
    };
    Rgb([(r * 255.) as u8, (g * 255.) as u8, (b * 255.) as u8])
}

// hue reflection algorithm
pub fn angle_reflect(reflect_angle: f32) -> impl Fn(f32)->f32 + Send {
    // for a hue angle C and reflection angle A
    // align angle to the 0 degree line by subtracting reflection angle
    // subtract from 360 to flip, add back reflection angle to realign
    // output angle is 360-(C-A)+A mod 360
    // or, 360-C+2A mod 360
    // and with algebra, 2A-C mod 360
    move |x| (2.0*reflect_angle - x).rem_euclid(360.0)
}

pub fn linear_piece_any(points: Vec<(f32, f32)>) -> impl Fn(f32)->f32 + Send {
    let mut points = points.clone();
    if points.is_empty() {
        points.push((180., 180.));
    }
    points.sort_by(|a, b| a.0.total_cmp(&b.0)); // order by x coordinate
    points.insert(0, (points[points.len()-1].0 - 360.0, points[points.len()-1].1 - 360.0)); // shift the last point leftward to before the modulo bounds
    points.push((points[0].0 + 360.0, points[0].1 + 360.0)); // shift the first point rightward to after the modulo bounds
    let len = points.len();
    let points = points;

    let mut slopes: Vec<(f32,f32)> = Vec::new(); // the first and last slopes go over the modulo
    let mut prev_point: (f32, f32) = points[0];
    for point in &points[1..len] { // compute the slope between every point, and bias using the first point as reference
        let slope: f32 = (point.1 - prev_point.1) / (point.0 - prev_point.0);
        let bias = prev_point.1 - slope * prev_point.0;
        slopes.push((slope, bias));
        prev_point = *point;
    }
    let slopes = slopes;

    move |x| {
        let mut index: usize = 0;
        for i in 0..len-1 {
            if x >= points[i].0 && x < points[i+1].0 {
                index = i; break;
            }
        }
        (x * slopes[index].0 + slopes[index].1).rem_euclid(360.0)
    }
}

pub fn linear_piece_two(p1: (f32, f32), p2: (f32, f32)) -> impl Fn(f32)->f32 + Send {
    // takes in two points and creates a partwise linear function between them both ways, modulo 360
    let (a,b) = if p1.0 < p2.0 {(p1, p2)} else {(p2, p1)};
    let slope_lowhigh = (b.1-a.1)/(b.0-a.0);
    let slope_highlow = (a.1+360.-b.1)/(a.0+360.-b.0);
    let lowhigh_bias = a.1 - slope_lowhigh * a.0;
    let highlow_bias_first = a.1 - slope_highlow * a.0;
    let highlow_bias_second = b.1 - slope_highlow * b.0;
    move |x| {
        if x < a.0 {
            (x * slope_highlow + highlow_bias_first).rem_euclid(360.)
        } else if (a.0 ..b.0).contains(&x) {
            (x * slope_lowhigh + lowhigh_bias).rem_euclid(360.)
        } else {
            (x * slope_highlow + highlow_bias_second).rem_euclid(360.)
        }
    }
}

// swap the two largest or smallest color channels
// true for largest
// false for smallest
pub fn rgb_conjugate(pixel: &Rgb<u8>, minmax: bool) -> Rgb<u8> {
    let mut sorted: Vec<u8> = pixel.0.to_vec();
    sorted.sort();
    let sorted = sorted;

    let channel_position = pixel.0.iter().position(|&p| p == sorted[match minmax {true => 0, false => 2}]).unwrap();
    match channel_position {
        0 => Rgb([pixel.0[0], pixel.0[2], pixel.0[1]]), // swaps g and b
        1 => Rgb([pixel.0[2], pixel.0[1], pixel.0[0]]), // swaps r and b
        2 => Rgb([pixel.0[1], pixel.0[0], pixel.0[2]]), // swaps r and g
        _ => Rgb([0, 0, 0]),
    }
}

pub fn inputf32() -> f32 {
    loop {
        let mut value = String::new();

        io::stdin()
            .read_line(&mut value)
            .expect("Failed to read line");

        match value.trim().parse() {
            Ok(num) => return num,
            Err(_) => continue,
        };
    }
}

pub fn inputi8() -> i8 {
    loop {
        let mut value = String::new();

        io::stdin()
            .read_line(&mut value)
            .expect("Failed to read line");

        match value.trim().parse() {
            Ok(num) => return num,
            Err(_) => continue,
        };
    }
}

pub fn inputstr() -> String {
    loop {
        let mut value = String::new();

        io::stdin()
            .read_line(&mut value)
            .expect("Failed to read line");

        match value.trim().parse() {
            Ok(num) => return num,
            Err(_) => continue,
        };
    }
}