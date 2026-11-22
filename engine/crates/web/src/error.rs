#[cfg(target_arch = "wasm32")]
pub type WebError = wasm_bindgen::JsValue;
#[cfg(not(target_arch = "wasm32"))]
pub type WebError = String;

pub(crate) fn error(message: impl std::fmt::Display) -> WebError {
    let message = message.to_string();
    #[cfg(target_arch = "wasm32")]
    {
        js_sys::Error::new(&message).into()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        message
    }
}

pub(crate) fn validate_range(value: f32, min: f32, max: f32, name: &str) -> Result<(), WebError> {
    if !value.is_finite() || !(min..=max).contains(&value) {
        return Err(error(format!(
            "{name}必须为 {min}..={max} 范围内的有限数值"
        )));
    }
    Ok(())
}
