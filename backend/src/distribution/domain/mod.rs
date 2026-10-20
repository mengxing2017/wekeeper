use crate::business::{Result, require};
use std::collections::BTreeMap;
const KEYS: [&str; 4] = [
    "school.name",
    "post.title",
    "post.summary",
    "post.detail_url",
];
pub fn validate(body: &str) -> Result<()> {
    require(
        !body.trim().is_empty() && body.len() <= 8000,
        "模板不能为空或过长",
    )?;
    let mut rest = body;
    while let Some(i) = rest.find("{{") {
        require(!rest[..i].contains("}}"), "模板括号不匹配")?;
        rest = &rest[i + 2..];
        let end = rest
            .find("}}")
            .ok_or_else(|| crate::business::Error::Invalid("模板括号不匹配".into()))?;
        require(KEYS.contains(&rest[..end].trim()), "模板包含未知占位符")?;
        rest = &rest[end + 2..];
    }
    require(!rest.contains("}}"), "模板括号不匹配")
}
pub fn render(body: &str, values: &BTreeMap<&str, String>) -> Result<String> {
    validate(body)?;
    let mut out = String::new();
    let mut rest = body;
    while let Some(i) = rest.find("{{") {
        out.push_str(&rest[..i]);
        rest = &rest[i + 2..];
        let end = rest.find("}}").expect("validated braces");
        let value = values
            .get(rest[..end].trim())
            .ok_or_else(|| crate::business::Error::Invalid("模板字段缺失".into()))?;
        require(!value.is_empty(), "模板字段为空")?;
        out.push_str(value);
        rest = &rest[end + 2..];
    }
    out.push_str(rest);
    Ok(out)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_expressions_and_bad_braces() {
        assert!(validate("{{system.exec}}").is_err());
        assert!(validate("{{post.title").is_err());
    }
    #[test]
    fn content_is_not_executed_as_a_template() {
        let values = BTreeMap::from([("post.title", "{{system.exec}}".into())]);
        assert_eq!(
            render("标题：{{post.title}}", &values).unwrap(),
            "标题：{{system.exec}}"
        );
    }
}
