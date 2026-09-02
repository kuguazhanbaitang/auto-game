//! 识别后端：模板匹配（基于 imageproc 纯 Rust 实现，含金字塔加速）
//!
//! 加速策略（针对 M1 实测全屏/大区域 NCC 慢的痛点）：
//! - 图像金字塔粗到细：先在低分辨率全图粗定位 top-K 候选，再逐层在候选邻域精匹配，
//!   把「全屏大匹配」变成「若干小窗口匹配」，语义仍是返回全局最高置信度位置；
//! - Rayon 并行：每层对 K 个候选窗口并行精匹配。

use anyhow::{Result, anyhow};
use image::{DynamicImage, GrayImage, Luma, RgbaImage};
use imageproc::template_matching::{find_extremes, match_template, MatchTemplateMethod};
use rayon::prelude::*;

/// 识别后端
pub struct VisionBackend;

/// 一次匹配结果
#[derive(Debug, Clone)]
pub struct Match {
    /// 匹配到的左上角 x 坐标（屏幕坐标）
    pub x: i32,
    /// 匹配到的左上角 y 坐标（屏幕坐标）
    pub y: i32,
    /// 模板宽度（用于计算中心点）
    pub width: u32,
    /// 模板高度
    pub height: u32,
    /// 置信度（0.0 ~ 1.0）
    pub confidence: f64,
}

impl Match {
    /// 模板中心点坐标（点击用）
    pub fn center(&self) -> (i32, i32) {
        (
            self.x + self.width as i32 / 2,
            self.y + self.height as i32 / 2,
        )
    }
}

/// 识别抽象契约
pub trait VisionTrait {
    /// 在屏幕图像中查找模板，返回最佳匹配（无匹配返回 None）
    fn find_template(
        &self,
        screen: &RgbaImage,
        template: &RgbaImage,
        precision: f64,
    ) -> Result<Option<Match>>;
}

impl VisionTrait for VisionBackend {
    fn find_template(
        &self,
        screen: &RgbaImage,
        template: &RgbaImage,
        precision: f64,
    ) -> Result<Option<Match>> {
        if template.width() > screen.width() || template.height() > screen.height() {
            return Err(anyhow!(
                "模板尺寸大于屏幕图像（模板 {}x{} vs 屏幕 {}x{}）",
                template.width(),
                template.height(),
                screen.width(),
                screen.height()
            ));
        }

        // 转灰度后做归一化交叉相关匹配
        let screen_gray = DynamicImage::ImageRgba8(screen.clone()).to_luma8();
        let template_gray = DynamicImage::ImageRgba8(template.clone()).to_luma8();

        // 小图/小模板用精确匹配（金字塔收益低且有降采样损失）
        let use_fast = template.width() >= 16
            && template.height() >= 16
            && screen.width() >= 128
            && screen.height() >= 128;
        let found = if use_fast {
            match_template_fast(&screen_gray, &template_gray, precision as f32)
        } else {
            match_template_exact(&screen_gray, &template_gray)
                .filter(|(_, _, s)| *s >= precision as f32)
        };

        match found {
            Some((x, y, score)) => Ok(Some(Match {
                x: x as i32,
                y: y as i32,
                width: template.width(),
                height: template.height(),
                confidence: score as f64,
            })),
            None => Ok(None),
        }
    }
}

/// 精确匹配：全图单次 NCC，返回 (x, y, confidence)
pub fn match_template_exact(screen: &GrayImage, template: &GrayImage) -> Option<(u32, u32, f32)> {
    if template.width() > screen.width() || template.height() > screen.height() {
        return None;
    }
    let result = match_template(
        screen,
        template,
        MatchTemplateMethod::CrossCorrelationNormalized,
    );
    let e = find_extremes(&result);
    Some((e.max_value_location.0, e.max_value_location.1, e.max_value))
}

// ---------- 加速：图像金字塔粗到细 + 并行候选精匹配 ----------

/// 金字塔下采样停止条件：屏幕最短边下限（低于则不再下采样）
const PYRAMID_MIN_SCREEN_SIDE: u32 = 48;
/// 模板最短边下限（模板太小下采样会丢失纹理）
const PYRAMID_MIN_TEMPLATE_SIDE: u32 = 12;
/// 每层保留候选数
const TOP_K: usize = 10;
/// 粗层候选的最低置信度底线（防止漏检）
const CAND_MIN_SCORE: f32 = 0.35;

fn downsample(img: &GrayImage) -> GrayImage {
    let w = (img.width() / 2).max(1);
    let h = (img.height() / 2).max(1);
    image::imageops::resize(img, w, h, image::imageops::FilterType::Triangle)
}

/// 取结果矩阵中置信度 ≥ min_score 的 top-k 位置（行优先遍历）
fn topk(result: &image::ImageBuffer<Luma<f32>, Vec<f32>>, k: usize, min_score: f32) -> Vec<(u32, u32, f32)> {
    let w = result.width();
    let mut v: Vec<(u32, u32, f32)> = result
        .pixels()
        .enumerate()
        .filter_map(|(i, px)| {
            let s = px[0];
            if s >= min_score {
                Some((i as u32 % w, i as u32 / w, s))
            } else {
                None
            }
        })
        .collect();
    v.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));
    v.truncate(k);
    v
}

/// 在 screen 的 (cx, cy) 邻域匹配模板，返回 screen 坐标下的最佳匹配。
///
/// 候选 (cx, cy) 是「模板左上角」的粗估位置；精匹配窗口取
/// 「模板尺寸 + 2×radius」，让模板左上角在窗口内可滑动 radius 像素，
/// 从而既保证窗口 ≥ 模板（match 可行），又能覆盖真实位置。
fn match_local(
    screen: &GrayImage,
    template: &GrayImage,
    cx: i64,
    cy: i64,
    radius: i64,
    min_score: f32,
) -> Option<(u32, u32, f32)> {
    let tw = template.width() as i64;
    let th = template.height() as i64;
    let sw = screen.width() as i64;
    let sh = screen.height() as i64;
    let win_w = (tw + 2 * radius).min(sw);
    let win_h = (th + 2 * radius).min(sh);
    let x0 = (cx - radius).clamp(0, sw - win_w);
    let y0 = (cy - radius).clamp(0, sh - win_h);
    let window =
        image::imageops::crop_imm(screen, x0 as u32, y0 as u32, win_w as u32, win_h as u32)
            .to_image();
    match_template_exact(&window, template)
        .filter(|(_, _, s)| *s >= min_score)
        .map(|(mx, my, s)| (x0 as u32 + mx, y0 as u32 + my, s))
}

/// 去重取 top-k：位置相近（<4px）的候选合并，保留高分
fn dedup_topk(v: Vec<(u32, u32, f32)>, k: usize) -> Vec<(u32, u32, f32)> {
    let mut v = v;
    v.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));
    let mut out: Vec<(u32, u32, f32)> = Vec::new();
    for m in v {
        let dup = out
            .iter()
            .any(|o| (o.0 as i64 - m.0 as i64).abs() < 4 && (o.1 as i64 - m.1 as i64).abs() < 4);
        if !dup {
            out.push(m);
        }
        if out.len() >= k {
            break;
        }
    }
    out
}

/// 金字塔粗到细模板匹配：先低分辨率全图粗定位，再逐层并行精匹配
pub fn match_template_fast(
    screen: &GrayImage,
    template: &GrayImage,
    precision: f32,
) -> Option<(u32, u32, f32)> {
    // 构建金字塔（screen 与 template 同步下采样，保持相对尺寸）
    let mut screens = vec![screen.clone()];
    let mut templates = vec![template.clone()];
    loop {
        let s = screens.last().unwrap();
        let t = templates.last().unwrap();
        if s.width().min(s.height()) < PYRAMID_MIN_SCREEN_SIDE
            || t.width().min(t.height()) < PYRAMID_MIN_TEMPLATE_SIDE
        {
            break;
        }
        screens.push(downsample(s));
        templates.push(downsample(t));
    }
    let top = screens.len() - 1;
    if top == 0 {
        // 不值得下采样：直接精确匹配
        return match_template_exact(screen, template).filter(|(_, _, s)| *s >= precision);
    }

    // 粗层底线：低于用户阈值太多，避免下采样导致的分数下降而漏检
    let coarse_min = (precision - 0.2).max(CAND_MIN_SCORE);

    // 最粗层全图匹配取 top-k 候选
    let result = match_template(
        &screens[top],
        &templates[top],
        MatchTemplateMethod::CrossCorrelationNormalized,
    );
    let mut cands = topk(&result, TOP_K, coarse_min);
    if cands.is_empty() {
        return None;
    }

    // 逐层细化：候选坐标 ×2 映射到本层，在邻域并行精匹配
    for lvl in (0..top).rev() {
        let next: Vec<(u32, u32, f32)> = cands
            .par_iter()
            .filter_map(|(x, y, _)| {
                match_local(
                    &screens[lvl],
                    &templates[lvl],
                    (*x as i64) * 2,
                    (*y as i64) * 2,
                    3,
                    coarse_min,
                )
            })
            .collect();
        if next.is_empty() {
            return None;
        }
        cands = dedup_topk(next, TOP_K);
    }

    let (x, y, score) = cands[0];
    if score >= precision {
        Some((x, y, score))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    /// 带纹理的随机屏幕：固定种子生成随机灰度背景，并在 (at_x, at_y) 处
    /// **直接拷贝嵌入**模板图案（正相关，目标处 NCC≈1；背景其他处为随机噪声）
    fn synth_screen(
        w: u32,
        h: u32,
        seed: u64,
        tw: u32,
        th: u32,
        at_x: u32,
        at_y: u32,
    ) -> (GrayImage, GrayImage) {
        let mut rng = seed;
        let mut rnd = move || {
            rng = rng
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (rng >> 33) as u8
        };
        let mut screen = GrayImage::new(w, h);
        for (_, _, p) in screen.enumerate_pixels_mut() {
            *p = Luma([rnd()]);
        }
        let mut tpl = GrayImage::new(tw, th);
        for (_, _, p) in tpl.enumerate_pixels_mut() {
            *p = Luma([rnd()]);
        }
        for y in 0..th {
            for x in 0..tw {
                let v = *tpl.get_pixel(x, y);
                screen.put_pixel(at_x + x, at_y + y, v);
            }
        }
        (screen, tpl)
    }

    /// 纯随机屏幕（不含任何嵌入目标）
    fn plain_screen(w: u32, h: u32, seed: u64) -> GrayImage {
        let mut rng = seed;
        let mut rnd = move || {
            rng = rng
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (rng >> 33) as u8
        };
        let mut screen = GrayImage::new(w, h);
        for (_, _, p) in screen.enumerate_pixels_mut() {
            *p = Luma([rnd()]);
        }
        screen
    }

    #[test]
    fn fast_matches_same_as_exact_on_synthetic() {
        let (screen, tpl) = synth_screen(200, 160, 12345, 32, 24, 80, 60);
        let exact = match_template_exact(&screen, &tpl).expect("exact 应找到目标");
        let fast = match_template_fast(&screen, &tpl, 0.5).expect("fast 应找到目标");
        assert!(
            (fast.0 as i64 - exact.0 as i64).abs() <= 1
                && (fast.1 as i64 - exact.1 as i64).abs() <= 1,
            "位置不一致：fast={fast:?} exact={exact:?}"
        );
        assert!(
            (fast.2 - exact.2).abs() < 0.02,
            "置信度不一致：fast={} exact={}",
            fast.2,
            exact.2
        );
    }

    #[test]
    fn fast_rejects_random_when_threshold_high() {
        let screen = plain_screen(160, 120, 777);
        let mut tpl = GrayImage::new(24, 18);
        let mut rng = 4242u64;
        for (_, _, p) in tpl.enumerate_pixels_mut() {
            rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            *p = Luma([(rng >> 33) as u8]);
        }
        // 屏幕与模板无关，高阈值下不应误报
        let r = match_template_fast(&screen, &tpl, 0.9);
        assert!(r.is_none(), "无关内容不应在 0.9 阈值下命中：{r:?}");
    }

    /// 耗时基准（手动运行观察加速比）：cargo test -- --ignored --nocapture
    #[test]
    #[ignore = "耗时基准，手动运行"]
    fn bench_fast_vs_exact() {
        let (screen, tpl) = synth_screen(400, 300, 2026, 50, 40, 180, 130);
        let t0 = Instant::now();
        let exact = match_template_exact(&screen, &tpl).unwrap();
        let t1 = Instant::now();
        let fast = match_template_fast(&screen, &tpl, 0.5).unwrap();
        let t2 = Instant::now();
        let exact_ms = (t1 - t0).as_millis();
        let fast_ms = (t2 - t1).as_millis();
        println!(
            "\n[bench] 400x300 + 50x40 模板   exact={exact_ms}ms   fast={fast_ms}ms   加速比≈{:.1}x",
            exact_ms as f64 / fast_ms.max(1) as f64
        );
        println!("[bench] exact={exact:?}  fast={fast:?}");
        assert!((fast.0 as i64 - exact.0 as i64).abs() <= 1);
    }

    /// 真实屏幕自匹配：取当前屏幕中一块 60x40 区域作模板，在原截图上用
    /// fast 路径匹配，应命中原位置且置信度≈1（验证真实 UI 图像下金字塔正常）。
    /// 注意：模板区域若为纯色/无纹理，NCC 会处处命中（已知坑），故先检测纹理方差再断言。
    #[test]
    #[ignore = "依赖真实屏幕，手动运行"]
    fn fast_on_real_screenshot() {
        use crate::adapter::{CaptureBackend, CaptureTrait};
        let screen = CaptureBackend.capture_full().expect("真实截图失败");
        let x0 = screen.width() / 2 - 30;
        let y0 = screen.height() / 2 - 20;
        let tpl = image::imageops::crop_imm(&screen, x0, y0, 60, 40).to_image();
        let sg = DynamicImage::ImageRgba8(screen).to_luma8();
        let tg = DynamicImage::ImageRgba8(tpl).to_luma8();

        // 纹理方差检测：纯色/无纹理区域（已知坑）跳过断言
        let n = (tg.width() * tg.height()) as f64;
        let (mut s1, mut s2) = (0f64, 0f64);
        for p in tg.pixels() {
            let v = p[0] as f64;
            s1 += v;
            s2 += v * v;
        }
        let var = s2 / n - (s1 / n) * (s1 / n);

        let t0 = Instant::now();
        let fast = match_template_fast(&sg, &tg, 0.95);
        let ms = (Instant::now() - t0).as_millis();
        println!(
            "\n[real] 目标({x0},{y0}) 60x40   fast={fast:?}   耗时={ms}ms   模板方差={var:.1}"
        );
        if var < 8.0 {
            println!("[real] 模板接近纯色（已知坑：NCC 处处命中），跳过位置断言");
            return;
        }
        match fast {
            Some((x, y, s)) => {
                assert!(
                    (x as i64 - x0 as i64).abs() <= 1 && (y as i64 - y0 as i64).abs() <= 1,
                    "真实截图自匹配位置偏移过大 ({x},{y}) vs ({x0},{y0})"
                );
                assert!(s > 0.99, "自匹配置信度应接近 1，实际 {s}");
            }
            None => panic!("真实截图自匹配不应失败"),
        }
    }
}
