use crate::{List, Value, eval::Evaluator, parse::Number, parse::Span};

use super::{BuiltinNamespace, BuiltinSpec};

macro_rules! unary_builtins {
    ($($name:ident => $method:ident),* $(,)?) => {
        const MEMBERS: &[BuiltinSpec] = &[
            BuiltinSpec::new("toBool", 1, to_bool),
            BuiltinSpec::new("atan2", 2, atan2),
            $(BuiltinSpec::new(stringify!($name), 1, $name),)*
            BuiltinSpec::new("HSL2RGB", 1, hsl2rgb),
            BuiltinSpec::new("HSV2RGB", 1, hsv2rgb),
            BuiltinSpec::new("RGB2HSL", 1, rgb2hsl),
            BuiltinSpec::new("RGB2HSV", 1, rgb2hsv),
        ];

        $(fn $name(e: &mut Evaluator<'_, '_>, a: &[Value], _: Span) -> Result<Value, String> {
            float_unary(e, &a[0], stringify!($name), f32::$method)
        })*
    };
}

unary_builtins! {
    acos => acos, asin => asin, atan => atan, cos => cos, log => ln,
    log10 => log10, sin => sin, sqrt => sqrt, tan => tan,
}

pub(super) const NAMESPACE: BuiltinNamespace = BuiltinNamespace::new("math", MEMBERS);

fn float_unary(
    evaluator: &mut Evaluator<'_, '_>,
    value: &Value,
    name: &'static str,
    op: fn(f32) -> f32,
) -> Result<Value, String> {
    evaluator.apply_unary(
        |value| {
            let Value::Number(value) = value else {
                return Err(format!("math.{name}: expected float"));
            };
            Ok(Value::Number(Number::Float(op(value.to_f32_lossy()))))
        },
        value.clone(),
    )
}

fn atan2(
    _evaluator: &mut Evaluator<'_, '_>,
    arguments: &[Value],
    _call_span: Span,
) -> Result<Value, String> {
    let (Value::Number(y), Value::Number(x)) = (&arguments[0], &arguments[1]) else {
        return Err("math.atan2: expected floats".into());
    };
    Ok(Value::Number(Number::Float(
        y.to_f32_lossy().atan2(x.to_f32_lossy()),
    )))
}

fn to_bool(
    evaluator: &mut Evaluator<'_, '_>,
    arguments: &[Value],
    _call_span: Span,
) -> Result<Value, String> {
    evaluator.apply_unary(
        |value| {
            Ok(Value::Bool(match value {
                Value::Bool(value) => value,
                Value::Number(Number::Int(value)) => value != 0,
                Value::Number(Number::Float(value)) => value != 0.0,
                Value::Str(value) if value.eq_ignore_ascii_case("true") => true,
                Value::Str(value) if value.eq_ignore_ascii_case("false") => false,
                _ => return Err("math.toBool: invalid value".into()),
            }))
        },
        arguments[0].clone(),
    )
}

fn color(value: &Value, name: &str, convert: fn([f32; 3]) -> [f32; 3]) -> Result<Value, String> {
    let error = || format!("math.{name}: expected float[3 or 4]");
    let Value::List(values) = value else {
        return Err(error());
    };
    let numbers = match values.as_slice() {
        [Value::Number(a), Value::Number(b), Value::Number(c)] => [*a, *b, *c, Number::Int(0)],
        [
            Value::Number(a),
            Value::Number(b),
            Value::Number(c),
            Value::Number(alpha),
        ] => [*a, *b, *c, *alpha],
        _ => return Err(error()),
    };
    let mut result = convert([numbers[0], numbers[1], numbers[2]].map(Number::to_f32_lossy))
        .into_iter()
        .map(|value| Value::Number(Number::Float(value)))
        .collect::<Vec<_>>();
    if values.as_slice().len() == 4 {
        result.push(Value::Number(Number::Float(numbers[3].to_f32_lossy())));
    }
    Ok(Value::List(List::from(result)))
}

fn hue_rgb([h, c, m]: [f32; 3]) -> [f32; 3] {
    let h = (h / 60.0).rem_euclid(6.0);
    let x = c * (1.0 - (h.rem_euclid(2.0) - 1.0).abs());
    let rgb = match h as u8 {
        0 => [c, x, 0.0],
        1 => [x, c, 0.0],
        2 => [0.0, c, x],
        3 => [0.0, x, c],
        4 => [x, 0.0, c],
        _ => [c, 0.0, x],
    };
    rgb.map(|value| value + m)
}

fn rgb_hue(r: f32, g: f32, b: f32, max: f32, delta: f32) -> f32 {
    if delta == 0.0 {
        0.0
    } else if max == r {
        60.0 * ((g - b) / delta).rem_euclid(6.0)
    } else if max == g {
        60.0 * ((b - r) / delta + 2.0)
    } else {
        60.0 * ((r - g) / delta + 4.0)
    }
}

fn hsl([h, s, l]: [f32; 3]) -> [f32; 3] {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    hue_rgb([h, c, l - c / 2.0])
}

fn hsv([h, s, v]: [f32; 3]) -> [f32; 3] {
    let c = v * s;
    hue_rgb([h, c, v - c])
}

fn to_hsl([r, g, b]: [f32; 3]) -> [f32; 3] {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;
    let l = (max + min) / 2.0;
    [
        rgb_hue(r, g, b, max, delta),
        if delta == 0.0 {
            0.0
        } else {
            delta / (1.0 - (2.0 * l - 1.0).abs())
        },
        l,
    ]
}

fn to_hsv([r, g, b]: [f32; 3]) -> [f32; 3] {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;
    [
        rgb_hue(r, g, b, max, delta),
        if max == 0.0 { 0.0 } else { delta / max },
        max,
    ]
}

macro_rules! color_builtin {
    ($name:ident, $display:literal, $convert:ident) => {
        fn $name(
            _evaluator: &mut Evaluator<'_, '_>,
            arguments: &[Value],
            _call_span: Span,
        ) -> Result<Value, String> {
            color(&arguments[0], $display, $convert)
        }
    };
}

color_builtin!(hsl2rgb, "HSL2RGB", hsl);
color_builtin!(hsv2rgb, "HSV2RGB", hsv);
color_builtin!(rgb2hsl, "RGB2HSL", to_hsl);
color_builtin!(rgb2hsv, "RGB2HSV", to_hsv);
