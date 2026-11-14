//! A shader the user writes, run on every window as it is drawn.
//!
//! The user's file defines one function and the compositor builds the rest of the shader
//! around it, so the rounded corners, the clip and the opacity stay as the style says and the
//! file cannot break them:
//!
//! ```glsl
//! vec4 effect(vec4 color, vec2 uv, vec2 size, float progress) {
//!     // color is the window's pixel, already multiplied by its opacity (premultiplied alpha)
//!     // uv runs from 0 to 1 across the window, size is the window in physical pixels
//!     // progress is 0 to 1 while the window's style glides (it appears, or its opacity or
//!     // radius changes) and 1 when it rests
//!     return color;
//! }
//! ```
//!
//! `tex` (the window's picture, sampled at `uv`) can be read too, for effects that look at
//! neighbouring pixels. GLSL ES 1.00 has no recursion and only loops with a fixed count, so
//! the cost per pixel is bounded by what is written; the size of the file is limited as well.
//! The file is the user's own, read from their configuration, with the same trust as it. Only
//! the directives of the preprocessor that make sense in a function are allowed: the version
//! and the extensions are the compositor's to choose.

use crate::look::WINDOW_SHADER;

/// The most a user's effect may be, in bytes. A shader of this size is already a program.
pub const MAX_EFFECT_BYTES: usize = 16 * 1024;

/// What is wrong with a user's effect.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EffectError {
    /// The source is empty or longer than [`MAX_EFFECT_BYTES`].
    #[error("the effect must be from 1 to {MAX_EFFECT_BYTES} bytes")]
    Size,
    /// The source has a byte that is not ASCII. GLSL ES 1.00 is written in ASCII, and a
    /// driver may refuse anything else, even in a comment.
    #[error("the effect may only hold ASCII characters")]
    NotAscii,
    /// A preprocessor line the compositor keeps for itself, with the line's number.
    #[error("line {line}: `{directive}` is not allowed in an effect")]
    Directive {
        /// The line, counting from 1.
        line: usize,
        /// The directive, such as `#version`.
        directive: String,
    },
    /// The source does not define `vec4 effect(...)`.
    #[error("the effect must define `vec4 effect(vec4 color, vec2 uv, vec2 size, float progress)`")]
    NoEffect,
    /// The driver refused the finished shader, with its message.
    #[error("the shader does not compile: {0}")]
    Compile(String),
}

/// The directives an effect may use.
const ALLOWED: [&str; 8] = [
    "define", "undef", "if", "ifdef", "ifndef", "else", "elif", "endif",
];

/// Checks the user's `source` against the rules above, without a driver.
///
/// # Errors
///
/// Returns the first rule it breaks.
///
/// # Examples
///
/// ```
/// use lutece_wayland::{EffectError, check_effect};
///
/// let fine = "vec4 effect(vec4 color, vec2 uv, vec2 size, float progress) { return color; }";
/// assert!(check_effect(fine).is_ok());
/// assert_eq!(check_effect(""), Err(EffectError::Size));
/// assert!(matches!(
///     check_effect("#version 300 es\nvec4 effect(vec4 c, vec2 u, vec2 s, float p) { return c; }"),
///     Err(EffectError::Directive { line: 1, .. })
/// ));
/// ```
pub fn check_effect(source: &str) -> Result<(), EffectError> {
    if source.trim().is_empty() || source.len() > MAX_EFFECT_BYTES {
        return Err(EffectError::Size);
    }
    if !source.is_ascii() {
        return Err(EffectError::NotAscii);
    }
    for (index, line) in source.lines().enumerate() {
        let Some(rest) = line.trim_start().strip_prefix('#') else {
            continue;
        };
        let word: String = rest
            .trim_start()
            .chars()
            .take_while(char::is_ascii_alphabetic)
            .collect();
        if !ALLOWED.contains(&word.as_str()) {
            return Err(EffectError::Directive {
                line: index + 1,
                directive: format!("#{word}"),
            });
        }
    }
    if !source.contains("vec4 effect(") {
        return Err(EffectError::NoEffect);
    }
    Ok(())
}

/// The window shader with the user's `source` built in: what the driver is given.
///
/// # Errors
///
/// Returns the rule `source` breaks (see [`check_effect`]).
pub fn window_effect_source(source: &str) -> Result<String, EffectError> {
    check_effect(source)?;
    let declarations = format!("uniform float progress;\n#line 1\n{source}\n#line 1000\n");
    Ok(WINDOW_SHADER
        .replacen("//_EFFECT_DECL_", &declarations, 1)
        .replacen(
            "//_EFFECT_CALL_",
            "color = effect(color, v_coords, clip_rect.zw, progress);",
            1,
        ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use std::process::{Command, Stdio};

    const IDENTITY: &str =
        "vec4 effect(vec4 color, vec2 uv, vec2 size, float progress) { return color; }";

    /// Effects of the kinds users write: all of them must build into a valid shader.
    const SAMPLES: [&str; 4] = [
        IDENTITY,
        // Colour comes in as the window appears.
        "vec4 effect(vec4 color, vec2 uv, vec2 size, float progress) {
            float grey = dot(color.rgb, vec3(0.299, 0.587, 0.114));
            return vec4(mix(vec3(grey), color.rgb, progress), color.a);
        }",
        // A wipe from the left.
        "vec4 effect(vec4 color, vec2 uv, vec2 size, float progress) {
            return uv.x <= progress ? color : vec4(0.0);
        }",
        // A blur by hand, with a fixed loop and a read of the picture itself.
        "#define TAPS 4
        vec4 effect(vec4 color, vec2 uv, vec2 size, float progress) {
            vec4 sum = color;
            for (int i = 1; i <= TAPS; i++) {
                float d = float(i) / size.x * (1.0 - progress);
                sum += texture2D(tex, uv + vec2(d, 0.0)) + texture2D(tex, uv - vec2(d, 0.0));
            }
            return sum / float(2 * TAPS + 1);
        }",
    ];

    /// Runs the reference GLSL compiler on `shader`, if it is installed. `None` when it is not.
    fn validated(shader: &str) -> Option<Result<(), String>> {
        let dir = tempfile::tempdir().ok()?;
        let path = dir.path().join("window.frag");
        std::fs::write(&path, shader).ok()?;
        let run = Command::new("glslangValidator")
            .arg("-S")
            .arg("frag")
            .arg(&path)
            .stdin(Stdio::null())
            .output()
            .ok()?;
        Some(if run.status.success() {
            Ok(())
        } else {
            Err(String::from_utf8_lossy(&run.stdout).into_owned())
        })
    }

    fn require_validator(outcome: Option<Result<(), String>>) {
        match outcome {
            Some(result) => assert_eq!(result, Ok(())),
            None => {
                assert!(
                    std::env::var_os("LUTECE_REQUIRE_GLSLANG").is_none(),
                    "glslangValidator is required (LUTECE_REQUIRE_GLSLANG is set) but is not installed"
                );
                eprintln!("skipped: glslangValidator is not installed");
            }
        }
    }

    #[test]
    fn the_window_shader_as_it_was_still_validates() {
        require_validator(validated(WINDOW_SHADER));
    }

    #[test]
    fn effects_of_the_usual_kinds_build_into_valid_shaders() {
        for sample in SAMPLES {
            let built = window_effect_source(sample).expect("the rules allow it");
            require_validator(validated(&built));
        }
    }

    #[test]
    fn the_effect_is_called_after_the_opacity_and_before_the_corners() {
        let built = window_effect_source(IDENTITY).expect("allowed");
        let called = built.find("color = effect(").expect("called");
        let opacity = built.find("color * alpha").expect("alpha");
        let corners = built.find("clamp(0.5 - d").expect("corners");
        assert!(opacity < called && called < corners);
        assert!(!built.contains("//_EFFECT"), "every marker is replaced");
    }

    #[test]
    fn what_the_compositor_keeps_for_itself_is_refused() {
        for directive in [
            "#version 300 es",
            "#extension GL_OES_standard_derivatives : enable",
            "#pragma optimize(off)",
            "#line 5",
            "#error nope",
            "  #  version 100",
        ] {
            let source = format!("{IDENTITY}\n{directive}\n");
            assert!(
                matches!(
                    check_effect(&source),
                    Err(EffectError::Directive { line: 2, .. })
                ),
                "{directive}"
            );
        }
        assert!(check_effect(&format!("#ifdef GL_ES\n#define X 1\n#endif\n{IDENTITY}")).is_ok());
    }

    #[test]
    fn a_file_with_no_effect_function_or_with_other_characters_is_refused() {
        assert_eq!(check_effect("void helper() {}"), Err(EffectError::NoEffect));
        assert_eq!(
            check_effect(&format!("// café\n{IDENTITY}")),
            Err(EffectError::NotAscii)
        );
        assert_eq!(check_effect(" \n\t"), Err(EffectError::Size));
    }

    proptest! {
        /// A source over the limit is refused however it is made.
        #[test]
        fn a_source_past_the_limit_is_refused(extra in 1usize..2000) {
            let source = format!("{IDENTITY}\n//{}", "x".repeat(MAX_EFFECT_BYTES + extra));
            prop_assert_eq!(check_effect(&source), Err(EffectError::Size));
        }

        /// Whatever precedes it, a version line is always found, and on the right line.
        #[test]
        fn a_version_line_is_found_wherever_it_is(
            before in prop::collection::vec("[a-z ;(){}=0-9.]{0,30}", 0..6),
        ) {
            let line = before.len() + 1;
            let mut lines: Vec<String> = before.iter().map(|l| format!("// {l}")).collect();
            lines.push("#version 100".to_owned());
            lines.push(IDENTITY.to_owned());
            let at = match check_effect(&lines.join("\n")) {
                Err(EffectError::Directive { line, .. }) => Some(line),
                _ => None,
            };
            prop_assert_eq!(at, Some(line));
        }

        /// Checking never panics, for any text.
        #[test]
        fn checking_any_text_gives_an_answer(text in ".{0,200}") {
            let _ = check_effect(&text);
        }
    }
}
