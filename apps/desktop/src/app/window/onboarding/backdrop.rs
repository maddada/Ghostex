//! The stage's two animated backgrounds, drawn on the CPU: the faint blue nebula behind the whole
//! stage (packages/core-ui/onboarding/nebula.tsx (deleted 2026-10-01)) and the dark veil behind the preview column
//! (dark-veil.tsx), plus the static vignette.
//!
//! CDXC:Onboarding 2026-09-28 WHY:
//! User decision for this port: reproduce the WebGL backgrounds natively so they look the same,
//! falling back to pre-rendered frames only if a live version is not feasible. GPUI has no custom
//! shader API, so both fragment shaders are evaluated here, line for line, into small bitmaps on a
//! background thread and the GPU scales them up (both are smooth fields, so a quarter-resolution
//! picture is indistinguishable once scaled and covered by the grain). The veil renders about 12
//! frames a second and the stage cross-fades between consecutive frames; the nebula drifts so slowly
//! (its clock runs at 3%) that a new frame every half second, also cross-faded, is continuous to the
//! eye (its stars twinkle over five seconds, so they cross-fade smoothly too). The film grain is
//! tiled into one bitmap once, because 160px tiles drawn side by side leave hairline seams when
//! scaled. No frames are shipped.
//! Differences that remain: the per-pixel ±0.5% noise of the veil is left out (it is below one 8-bit
//! step), and the nebula's value-noise hash runs in CPU float, so its cloud and star positions are
//! the same kind of pattern rather than the same pattern.

/// One evaluated frame, BGRA rows top to bottom, straight alpha.
pub(crate) struct BackdropPixels {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) bgra: Vec<u8>,
}

/// Veil bitmap size: a quarter of the veil box (945x941 stage px).
pub(crate) const VEIL_PIXELS: (u32, u32) = (236, 235);
/// Nebula bitmap size: half of the 836x470 canvas the React page drew into.
pub(crate) const NEBULA_PIXELS: (u32, u32) = (418, 235);
/// The veil's canvas aspect (its box is 945 x 941 stage px).
const VEIL_ASPECT: f32 = 945.0 / 941.0;
/// The nebula canvas the React page drew (836 x 470), which fixes its aspect and star grid.
const NEBULA_RES: (f32, f32) = (836.0, 470.0);

type M4 = [[f32; 4]; 4];

fn sigmoid4(v: [f32; 4]) -> [f32; 4] {
    [
        1.0 / (1.0 + (-v[0]).exp()),
        1.0 / (1.0 + (-v[1]).exp()),
        1.0 / (1.0 + (-v[2]).exp()),
        1.0 / (1.0 + (-v[3]).exp()),
    ]
}

/// `bias + sum(M * buf[i])`, where a GLSL `mat4(c0, c1, c2, c3)` is built from columns.
#[inline]
fn layer(buf: &[[f32; 4]; 8], terms: &[(M4, usize)], bias: &[f32; 4]) -> [f32; 4] {
    let mut out = *bias;
    for (matrix, source) in terms {
        let v = buf[*source];
        for (column, value) in matrix.iter().zip(v) {
            out[0] += column[0] * value;
            out[1] += column[1] * value;
            out[2] += column[2] * value;
            out[3] += column[3] * value;
        }
    }
    out
}

// The CPPN weights, transcribed from the GLSL in packages/core-ui/onboarding/dark-veil.tsx (deleted 2026-10-01).
const W0: M4 = [
    [6.5404263, -3.6126034, 0.7590882, -1.13613],
    [2.4582713, 3.1660357, 1.2219609, 0.06276096],
    [-5.478085, -6.159632, 1.8701609, -4.7742867],
    [6.039214, -5.542865, -0.90925294, 3.251348],
];
const W1: M4 = [
    [0.8473259, -5.722911, 3.975766, 1.6522468],
    [-0.24321538, 0.5839259, -1.7661959, -5.350116],
    [0.0, 0.0, 0.0, 0.0],
    [0.0, 0.0, 0.0, 0.0],
];
const B2: [f32; 4] = [0.21808943, 1.1243913, -1.7969975, 5.0294676];
const W3: M4 = [
    [-3.3522482, -6.0612736, 0.55641043, -4.4719114],
    [0.8631464, 1.7432913, 5.643898, 1.6106541],
    [2.4941394, -3.5012043, 1.7184316, 6.357333],
    [3.310376, 8.209261, 1.1355612, -1.165539],
];
const W4: M4 = [
    [5.24046, -13.034365, 0.009859298, 15.870829],
    [2.987511, 3.129433, -0.89023495, -1.6822904],
    [0.0, 0.0, 0.0, 0.0],
    [0.0, 0.0, 0.0, 0.0],
];
const B5: [f32; 4] = [-5.9457836, -6.573602, -0.8812491, 1.5436668];
const W6: M4 = [
    [-15.219568, 8.095543, -2.429353, -1.9381982],
    [-5.951362, 4.3115187, 2.6393783, 1.274315],
    [-7.3145227, 6.7297835, 5.2473326, 5.9411426],
    [5.0796127, 8.979051, -1.7278991, -1.158976],
];
const W7: M4 = [
    [-11.967154, -11.608155, 6.1486754, 11.237008],
    [2.124141, -6.263192, -1.7050359, -0.7021966],
    [0.0, 0.0, 0.0, 0.0],
    [0.0, 0.0, 0.0, 0.0],
];
const B8: [f32; 4] = [-4.17164, -3.2281182, -4.576417, -3.6401186];
const W9: M4 = [
    [3.1832156, -13.738922, 1.879223, 3.233465],
    [0.64300746, 12.768129, 1.9141049, 0.50990224],
    [-0.049295485, 4.4807224, 1.4733979, 1.801449],
    [5.0039253, 13.000481, 3.3991797, -4.5561905],
];
const W10: M4 = [
    [-0.1285731, 7.720628, -3.1425676, 4.742367],
    [0.6393625, 3.714393, -0.8108378, -0.39174938],
    [0.0, 0.0, 0.0, 0.0],
    [0.0, 0.0, 0.0, 0.0],
];
const B11: [f32; 4] = [-1.1811101, -21.621881, 0.7851888, 1.2329718];
const W12: M4 = [
    [5.214916, -7.183024, 2.7228765, 2.6592617],
    [-5.601878, -25.3591, 4.067988, 0.4602802],
    [-10.57759, 24.286327, 21.102104, 37.546658],
    [4.3024497, -1.9625226, 2.3458803, -1.372816],
];
const W13: M4 = [
    [-17.6526, -10.507558, 2.2587414, 12.462782],
    [6.265566, -502.75443, -12.642513, 0.9112289],
    [-10.983244, 20.741234, -9.701768, -0.7635988],
    [5.383626, 1.4819539, -4.1911616, -4.8444734],
];
const W14: M4 = [
    [12.785233, -16.345072, -0.39901125, 1.7955981],
    [-30.48365, -1.8345358, 1.4542528, -1.1118771],
    [19.872723, -7.337935, -42.941723, -98.52709],
    [8.337645, -2.7312303, -2.2927687, -36.142323],
];
const W15: M4 = [
    [-16.298317, 3.5471997, -0.44300047, -9.444417],
    [57.5077, -35.609753, 16.163465, -4.1534753],
    [-0.07470326, -3.8656476, -7.0901804, 3.1523974],
    [-12.559385, -7.077619, 1.490437, -0.8211543],
];
const B16: [f32; 4] = [-7.67914, 15.927437, 1.3207729, -1.6686112];
const W17: M4 = [
    [-1.4109162, -0.372762, -3.770383, -21.367174],
    [-6.2103205, -9.35908, 0.92529047, 8.82561],
    [11.460242, -22.348068, 13.625772, -18.693201],
    [-0.3429052, -3.9905605, -2.4626114, -0.45033523],
];
const W18: M4 = [
    [7.3481627, -4.3661838, -6.3037653, -3.868115],
    [1.5462853, 6.5488915, 1.9701879, -0.58291394],
    [6.5858274, -2.2180402, 3.7127688, -1.3730392],
    [-5.7973905, 10.134961, -2.3395722, -5.965605],
];
const W19: M4 = [
    [-2.5132585, -6.6685553, -1.4029363, -0.16285264],
    [-0.37908727, 0.53738135, 4.389061, -1.3024765],
    [-0.70647055, 2.0111287, -5.1659346, -3.728635],
    [-13.562562, 10.487719, -0.9173751, -2.6487076],
];
const W20: M4 = [
    [-8.645013, 6.5546675, -6.3944063, -5.5933375],
    [-0.57783127, -1.077275, 36.91025, 5.736769],
    [14.283112, 3.7146652, 7.1452246, -4.5958776],
    [2.7192075, 3.6021907, -4.366337, -2.3653464],
];
const B21: [f32; 4] = [-5.9000807, -4.329569, 1.2427121, 8.59503];
const W22: M4 = [
    [-1.61102, 0.7970257, 1.4675229, 0.20917463],
    [-28.793737, -7.1390953, 1.5025433, 4.656581],
    [-10.94861, 39.66238, 0.74318546, -10.095605],
    [-0.7229728, -1.5483948, 0.7301322, 2.1687684],
];
const W23: M4 = [
    [3.2547753, 21.489103, -1.0194173, -3.3100595],
    [-3.7316632, -3.3792162, -7.223193, -0.23685838],
    [13.1804495, 0.7916005, 5.338587, 5.687114],
    [-4.167605, -17.798311, -6.815736, -1.6451967],
];
const W24: M4 = [
    [0.604885, -7.800309, -7.213122, -2.741014],
    [-3.522382, -0.12359311, -0.5258442, 0.43852118],
    [9.6752825, -22.853785, 2.062431, 0.099892326],
    [-4.3196306, -17.730087, 2.5184598, 5.30267],
];
const W25: M4 = [
    [-6.545563, -15.790176, -6.0438633, -5.415399],
    [-43.591583, 28.551912, -16.00161, 18.84728],
    [4.212382, 8.394307, 3.0958717, 8.657522],
    [-5.0237565, -4.450633, -4.4768, -5.5010443],
];
const W26: M4 = [
    [1.6985557, -67.05806, 6.897715, 1.9004834],
    [1.8680354, 2.3915145, 2.5231109, 4.081538],
    [11.158006, 1.7294737, 2.0738268, 7.386411],
    [-4.256034, -306.24686, 8.258898, -17.132736],
];
const W27: M4 = [
    [1.6889864, -4.5852966, 3.8534803, -6.3482175],
    [1.3543309, -1.2640043, 9.932754, 2.9079645],
    [-5.2770967, 0.07150358, -0.13962056, 3.3269649],
    [28.34703, -4.918278, 6.1044083, 4.085355],
];
const B28: [f32; 4] = [6.6818056, 12.522166, -3.7075126, -4.104386];
const W29: M4 = [
    [-8.265602, -4.7027016, 5.098234, 0.7509808],
    [8.6507845, -17.15949, 16.51939, -8.884479],
    [-4.036479, -2.3946867, -2.6055532, -1.9866527],
    [-2.2167742, -1.8135649, -5.9759874, 4.8846445],
];
const W30: M4 = [
    [6.7790847, 3.5076547, -2.8191125, -2.7028968],
    [-5.743024, -0.27844876, 1.4958696, -5.0517144],
    [13.122226, 15.735168, -2.9397483, -4.101023],
    [-14.375265, -5.030483, -6.2599335, 2.9848232],
];
const W31: M4 = [
    [4.0950394, -0.94011575, -5.674733, 4.755022],
    [4.3809423, 4.8310084, 1.7425908, -3.437416],
    [2.117492, 0.16342592, -104.56341, 16.949184],
    [-5.22543, -2.994248, 3.8350096, -1.9364246],
];
const W32: M4 = [
    [-5.900337, 1.7946124, -13.604192, -3.8060522],
    [6.6583457, 31.911177, 25.164474, 91.81147],
    [11.840538, 4.1503043, -0.7314397, 6.768467],
    [-6.3967767, 4.034772, 6.1714606, -0.32874924],
];
const W33: M4 = [
    [3.4992442, -196.91893, -8.923708, 2.8142626],
    [3.4806502, -3.1846354, 5.1725626, 5.1804223],
    [-2.4009497, 15.585794, 1.2863957, 2.0252278],
    [-71.25271, -62.441242, -8.138444, 0.50670296],
];
const W34: M4 = [
    [-12.291733, -11.176166, -7.3474145, 4.390294],
    [10.805477, 5.6337385, -0.9385842, -4.7348723],
    [-12.869276, -7.039391, 5.3029537, 7.5436664],
    [1.4593618, 8.91898, 3.5101583, 5.840625],
];
const B35: [f32; 4] = [2.2415268, -6.705987, -0.98861027, -2.117676];
const W36: M4 = [
    [1.6794263, 1.3817469, 2.9625452, 0.0],
    [-1.8834411, -1.4806935, -3.5924516, 0.0],
    [-1.3279216, -1.0918057, -2.3124623, 0.0],
    [0.2662234, 0.23235129, 0.44178495, 0.0],
];
const W37: M4 = [
    [-0.6299101, -0.5945583, -0.9125601, 0.0],
    [0.17828953, 0.18300213, 0.18182953, 0.0],
    [-2.96544, -2.5819945, -4.9001055, 0.0],
    [1.4195864, 1.1868085, 2.5176322, 0.0],
];
const W38: M4 = [
    [-1.2584374, -1.0552157, -2.1688404, 0.0],
    [-0.7200217, -0.52666044, -1.438251, 0.0],
    [0.15345335, 0.15196142, 0.272854, 0.0],
    [0.945728, 0.8861938, 1.2766753, 0.0],
];
const W39: M4 = [
    [-2.4218085, -1.968602, -4.35166, 0.0],
    [-22.683098, -18.0544, -41.954372, 0.0],
    [0.63792, 0.5470648, 1.1078634, 0.0],
    [-1.5489894, -1.3075932, -2.6444845, 0.0],
];
const W40: M4 = [
    [-0.49252132, -0.39877754, -0.91366625, 0.0],
    [0.95609266, 0.7923952, 1.640221, 0.0],
    [0.30616966, 0.15693925, 0.8639857, 0.0],
    [1.1825981, 0.94504964, 2.176963, 0.0],
];
const W41: M4 = [
    [0.35446745, 0.3293795, 0.59547555, 0.0],
    [-0.58784515, -0.48177817, -1.0614829, 0.0],
    [2.5271258, 1.9991658, 4.6846647, 0.0],
    [0.13042648, 0.08864098, 0.30187556, 0.0],
];
const W42: M4 = [
    [-1.7718065, -1.4033192, -3.3355875, 0.0],
    [3.1664357, 2.638297, 5.378702, 0.0],
    [-3.1724713, -2.6107926, -5.549295, 0.0],
    [-2.851368, -2.249092, -5.3013067, 0.0],
];
const W43: M4 = [
    [1.5203838, 1.2212278, 2.8404984, 0.0],
    [1.5210563, 1.2651345, 2.683903, 0.0],
    [2.9789467, 2.4364579, 5.2347264, 0.0],
    [2.2270417, 1.8825914, 3.8028636, 0.0],
];
const B44: [f32; 4] = [-1.5468478, -3.6171484, 0.24762098, 0.0];

fn cppn(x: f32, y: f32, in0: f32, in1: f32, in2: f32) -> [f32; 3] {
    let mut buf = [[0.0f32; 4]; 8];
    buf[6] = [x, y, 0.394_833_3 + in0, 0.36 + in1];
    buf[7] = [0.14 + in2, (x * x + y * y).sqrt(), 0.0, 0.0];
    buf[0] = layer(&buf, &[(W0, 6), (W1, 7)], &B2);
    buf[1] = layer(&buf, &[(W3, 6), (W4, 7)], &B5);
    buf[0] = sigmoid4(buf[0]);
    buf[1] = sigmoid4(buf[1]);
    buf[2] = layer(&buf, &[(W6, 6), (W7, 7)], &B8);
    buf[3] = layer(&buf, &[(W9, 6), (W10, 7)], &B11);
    buf[2] = sigmoid4(buf[2]);
    buf[3] = sigmoid4(buf[3]);
    buf[4] = layer(&buf, &[(W12, 0), (W13, 1), (W14, 2), (W15, 3)], &B16);
    buf[5] = layer(&buf, &[(W17, 0), (W18, 1), (W19, 2), (W20, 3)], &B21);
    buf[4] = sigmoid4(buf[4]);
    buf[5] = sigmoid4(buf[5]);
    buf[6] = layer(
        &buf,
        &[(W22, 0), (W23, 1), (W24, 2), (W25, 3), (W26, 4), (W27, 5)],
        &B28,
    );
    buf[7] = layer(
        &buf,
        &[(W29, 0), (W30, 1), (W31, 2), (W32, 3), (W33, 4), (W34, 5)],
        &B35,
    );
    buf[6] = sigmoid4(buf[6]);
    buf[7] = sigmoid4(buf[7]);
    buf[0] = layer(
        &buf,
        &[
            (W36, 0),
            (W37, 1),
            (W38, 2),
            (W39, 3),
            (W40, 4),
            (W41, 5),
            (W42, 6),
            (W43, 7),
        ],
        &B44,
    );
    buf[0] = sigmoid4(buf[0]);
    [buf[0][0], buf[0][1], buf[0][2]]
}

/// GLSL's column-major `mat3 * vec3`.
fn mat3_mul(columns: [[f32; 3]; 3], v: [f32; 3]) -> [f32; 3] {
    [
        columns[0][0] * v[0] + columns[1][0] * v[1] + columns[2][0] * v[2],
        columns[0][1] * v[0] + columns[1][1] * v[1] + columns[2][1] * v[2],
        columns[0][2] * v[0] + columns[1][2] * v[1] + columns[2][2] * v[2],
    ]
}

const RGB2YIQ: [[f32; 3]; 3] = [
    [0.299, 0.587, 0.114],
    [0.596, -0.274, -0.322],
    [0.211, -0.523, 0.312],
];
const YIQ2RGB: [[f32; 3]; 3] = [
    [1.0, 0.956, 0.621],
    [1.0, -0.272, -0.647],
    [1.0, -1.106, 1.703],
];

fn hue_shift(color: [f32; 3], degrees: f32) -> [f32; 3] {
    let yiq = mat3_mul(RGB2YIQ, color);
    let (sin, cos) = degrees.to_radians().sin_cos();
    let shifted = [
        yiq[0],
        yiq[1] * cos - yiq[2] * sin,
        yiq[1] * sin + yiq[2] * cos,
    ];
    let rgb = mat3_mul(YIQ2RGB, shifted);
    [
        rgb[0].clamp(0.0, 1.0),
        rgb[1].clamp(0.0, 1.0),
        rgb[2].clamp(0.0, 1.0),
    ]
}

fn to_byte(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
}

/// The veil at shader time `time` (seconds since open times the 0.6 speed), with the modal's
/// settings: hue shift 25, warp 0.2, scanline intensity 1 at frequency 0 (a flat 0.75 dim), dim 0.8,
/// scale (0.75, 0.35), offset (0, -0.8) and saturation `VEIL_SATURATION`.
pub(crate) fn render_veil(time: f32, saturation: f32) -> BackdropPixels {
    let (width, height) = VEIL_PIXELS;
    let mut bgra = vec![0u8; (width * height * 4) as usize];
    let in0 = 0.1 * (0.3 * time).sin();
    let in1 = 0.1 * (0.69 * time).sin();
    let in2 = 0.1 * (0.44 * time).sin();
    let warp = 0.2f32;
    let scanline_dim = 1.0 - 0.25 * 1.0;
    for row in 0..height {
        // gl_FragCoord runs bottom-up; the shader flips y back, so the top row is uv.y = -1.
        let uv_y = ((row as f32 + 0.5) / height as f32) * 2.0 - 1.0;
        for column in 0..width {
            let mut u = (((column as f32 + 0.5) / width as f32) * 2.0 - 1.0) * VEIL_ASPECT;
            let mut v = uv_y;
            u *= 0.75;
            v = v * 0.35 - 0.8;
            let du = (v * std::f32::consts::TAU + time * 0.5).sin() * warp * 0.05;
            let dv = (u * std::f32::consts::TAU + time * 0.5).cos() * warp * 0.05;
            u += du;
            v += dv;
            let color = cppn(u, v, in0, in1, in2);
            let mut color = hue_shift(color, 25.0);
            let luma = color[0] * 0.2126 + color[1] * 0.7152 + color[2] * 0.0722;
            for channel in &mut color {
                *channel = (luma + (*channel - luma) * saturation) * 0.8 * scanline_dim;
            }
            let index = ((row * width + column) * 4) as usize;
            bgra[index] = to_byte(color[2]);
            bgra[index + 1] = to_byte(color[1]);
            bgra[index + 2] = to_byte(color[0]);
            bgra[index + 3] = 255;
        }
    }
    BackdropPixels {
        width,
        height,
        bgra,
    }
}

fn fract(value: f32) -> f32 {
    value - value.floor()
}

/// The shader's `fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453)` as a GPU evaluates it.
///
/// CDXC:Onboarding 2026-09-28 WHY:
/// The hash multiplies `sin` of arguments in the thousands by 43758, so its value depends on how
/// the GPU reduces the angle: the dot is a multiply and a fused multiply-add in f32, and the sine
/// unit reduces the f32 product of the angle and 1/(2π) to a fraction of a turn. Evaluating it the
/// same way puts the nebula's clouds and stars where the page drew them on a desktop GPU; an exact
/// f64 `sin` scattered them elsewhere.
fn hash(x: f32, y: f32) -> f32 {
    let dot = y.mul_add(311.7, x * 127.1);
    let turns = dot * 0.159_154_94;
    let sine = ((turns - turns.floor()) as f64 * std::f64::consts::TAU).sin() as f32;
    fract(sine * 43_758.547)
}

fn value_noise(x: f32, y: f32) -> f32 {
    let (ix, iy) = (x.floor(), y.floor());
    let (fx, fy) = (x - ix, y - iy);
    let (ux, uy) = (fx * fx * (3.0 - 2.0 * fx), fy * fy * (3.0 - 2.0 * fy));
    let a = hash(ix, iy);
    let b = hash(ix + 1.0, iy);
    let c = hash(ix, iy + 1.0);
    let d = hash(ix + 1.0, iy + 1.0);
    let top = a + (b - a) * ux;
    let bottom = c + (d - c) * ux;
    top + (bottom - top) * uy
}

fn fbm(mut x: f32, mut y: f32) -> f32 {
    let mut sum = 0.0;
    let mut amplitude = 0.5;
    for _ in 0..5 {
        sum += amplitude * value_noise(x, y);
        x = x * 2.03 + 1.7;
        y = y * 2.03 + 9.2;
        amplitude *= 0.5;
    }
    sum
}

fn band(px: f32, py: f32, cx: f32, cy: f32, radius: f32, width: f32) -> f32 {
    let d = ((px - cx) * (px - cx) + (py - cy) * (py - cy)).sqrt() - radius;
    (-d * d / (width * width)).exp()
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The nebula at shader time `t` (seconds; the React clock starts at 40), stars included.
pub(crate) fn render_nebula(t: f32) -> BackdropPixels {
    let (width, height) = NEBULA_PIXELS;
    let mut bgra = vec![0u8; (width * height * 4) as usize];
    let (res_x, res_y) = NEBULA_RES;
    let big_t = t * 0.03;
    let blue = {
        let blue = [0.12f32, 0.23, 0.82];
        let l = blue[0] * 0.2126 + blue[1] * 0.7152 + blue[2] * 0.0722;
        [
            blue[0] + (l * 1.6 - blue[0]) * 0.62,
            blue[1] + (l * 1.6 - blue[1]) * 0.62,
            blue[2] + (l * 1.6 - blue[2]) * 0.62,
        ]
    };
    let amt = 0.09f32;
    for row in 0..height {
        let ys = (row as f32 + 0.5) / height as f32;
        for column in 0..width {
            let vx = (column as f32 + 0.5) / width as f32;
            let px = vx * res_x / res_y;
            let py = ys;
            let wx = fbm(px * 1.6 + big_t, py * 1.6) - 0.5;
            let wy = fbm(px * 1.6 + 4.3, py * 1.6 - big_t) - 0.5;
            let qx = px + wx * 0.24;
            let qy = py + wy * 0.24;
            let mut s = 0.0;
            s += band(
                qx,
                qy,
                1.30 + 0.05 * (big_t * 1.3).sin(),
                -1.05,
                1.40,
                0.040,
            );
            s += band(qx, qy, 2.05, 1.70 + 0.05 * big_t.cos(), 1.12, 0.055) * 0.9;
            s += band(qx, qy, 0.35, 1.55, 0.95, 0.050) * 0.6;
            s += band(qx, qy, 1.62, 0.30 + 0.04 * (big_t * 1.7).sin(), 0.62, 0.028) * 0.55;
            s += band(qx, qy, 0.95, -0.35, 0.80, 0.035) * 0.5;
            let cl = fbm(qx * 2.2 - big_t * 0.7, qy * 2.2 + big_t * 0.4);
            s = s.min(1.1);
            let neb = s * (0.35 + 0.9 * cl) + smoothstep(0.5, 0.98, cl) * 0.24;
            let glow = s.powi(9) * 0.10 * amt;
            let mut color = [
                blue[0] * neb * amt * 1.3 + 0.55 * glow,
                blue[1] * neb * amt * 1.3 + 0.68 * glow,
                blue[2] * neb * amt * 1.3 + 1.0 * glow,
            ];
            let base = 0.044 - 0.014 * ys;
            let gx = vx - 0.05;
            let gy = ys - 0.06;
            let spot = (-(gx * gx / 0.10 + gy * gy / 0.34)).exp() * 0.026;
            color[0] += base * 0.88 + spot * 0.5;
            color[1] += base * 0.95 + spot * 0.7;
            color[2] += base * 1.05 + spot * 1.0;
            // The star field: one 2x2 canvas-pixel cell per bitmap pixel (the grid is bottom-up).
            let r = hash(column as f32, (height - 1 - row) as f32);
            let star = smoothstep(0.9978, 1.0, r);
            if star > 0.0 {
                let twinkle = star * (0.5 + 0.5 * (t * 1.2 + r * 400.0).sin()) * 0.18;
                color[0] += 0.75 * twinkle;
                color[1] += 0.82 * twinkle;
                color[2] += twinkle;
            }
            let index = ((row * width + column) * 4) as usize;
            bgra[index] = to_byte(color[2]);
            bgra[index + 1] = to_byte(color[1]);
            bgra[index + 2] = to_byte(color[0]);
            bgra[index + 3] = 255;
        }
    }
    BackdropPixels {
        width,
        height,
        bgra,
    }
}

/// `radial-gradient(ellipse 72% 78% at 50% 50%, transparent 58%, rgba(0,0,0,.3) 100%)`.
pub(crate) fn render_vignette() -> BackdropPixels {
    let (width, height) = (168u32, 95u32);
    let mut bgra = vec![0u8; (width * height * 4) as usize];
    for row in 0..height {
        let dy = ((row as f32 + 0.5) / height as f32 - 0.5) / 0.78;
        for column in 0..width {
            let dx = ((column as f32 + 0.5) / width as f32 - 0.5) / 0.72;
            let t = (dx * dx + dy * dy).sqrt();
            let alpha = ((t - 0.58) / (1.0 - 0.58)).clamp(0.0, 1.0) * 0.3;
            let index = ((row * width + column) * 4) as usize;
            bgra[index + 3] = to_byte(alpha);
        }
    }
    BackdropPixels {
        width,
        height,
        bgra,
    }
}

/// `.grain`: the 160px noise tile repeated across the stage, composed into one bitmap.
///
/// CDXC:Onboarding 2026-10-22 WHY: the tile is lossless WebP (pixel-identical to the PNG it replaced, a third of the bytes). Lossy WebP or JPEG would smear the per-pixel noise that is the whole point of the tile.
pub(crate) fn render_grain() -> Option<BackdropPixels> {
    let tile = image::load_from_memory_with_format(
        include_bytes!("../../../../../../packages/core-ui/onboarding/assets/grain.webp"),
        image::ImageFormat::WebP,
    )
    .ok()?
    .into_rgba8();
    let (tile_width, tile_height) = tile.dimensions();
    let (columns, rows) = (11u32, 6u32);
    let (width, height) = (tile_width * columns, tile_height * rows);
    let mut bgra = vec![0u8; (width * height * 4) as usize];
    for y in 0..height {
        for x in 0..width {
            let pixel = tile.get_pixel(x % tile_width, y % tile_height).0;
            let index = ((y * width + x) * 4) as usize;
            bgra[index] = pixel[2];
            bgra[index + 1] = pixel[1];
            bgra[index + 2] = pixel[0];
            bgra[index + 3] = pixel[3];
        }
    }
    Some(BackdropPixels {
        width,
        height,
        bgra,
    })
}

/// The grain bitmap covers 11 x 6 tiles of 160 stage px.
pub(crate) const GRAIN_STAGE_SIZE: (f32, f32) = (160.0 * 11.0, 160.0 * 6.0);
