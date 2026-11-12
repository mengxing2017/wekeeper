use serde::Serialize;
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct BusinessUnit {
    pub id: Uuid,
    pub platform_id: Uuid,
    pub name: String,
    pub active: bool,
    pub revision: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct BusinessPlatform {
    pub id: Uuid,
    pub name: String,
    pub status: String,
}

#[derive(Debug, Serialize)]
pub struct DirectoryPage<T> {
    pub items: Vec<T>,
    pub has_more: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub enum DirectoryError {
    Forbidden,
    InvalidPage,
    Unavailable,
}

pub type Result<T> = std::result::Result<T, DirectoryError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn directory_pages_match_shared_contract_fixture() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../api-contract/fixtures/business-directory.json"
        ))
        .unwrap();
        let platform = "11111111-1111-4111-8111-111111111111".parse().unwrap();
        let unit = "22222222-2222-4222-8222-222222222222".parse().unwrap();
        let platforms = DirectoryPage {
            items: vec![BusinessPlatform {
                id: platform,
                name: "示例平台".into(),
                status: "active".into(),
            }],
            has_more: false,
        };
        let units = DirectoryPage {
            items: vec![BusinessUnit {
                id: unit,
                platform_id: platform,
                name: "示例单位".into(),
                active: true,
                revision: 1,
            }],
            has_more: false,
        };
        assert_eq!(
            serde_json::to_value(platforms).unwrap(),
            fixture["platforms_page"]
        );
        assert_eq!(serde_json::to_value(units).unwrap(), fixture["units_page"]);
    }
}
