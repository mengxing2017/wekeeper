use serde_json::Value;

pub(super) fn facts_complete(conditions: &Value) -> bool {
    [
        "interactive",
        "keyguard_locked",
        "device_locked",
        "keyguard_secure",
    ]
    .iter()
    .all(|key| conditions[*key].is_boolean())
}
pub(super) fn state(conditions: &Value) -> Option<&'static str> {
    if !facts_complete(conditions) {
        return None;
    }
    if conditions["device_locked"] == true && conditions["keyguard_secure"] == true {
        Some("secure_locked")
    } else if conditions["interactive"] == false {
        Some("screen_off")
    } else if conditions["keyguard_locked"] == true || conditions["device_locked"] == true {
        Some("keyguard_locked")
    } else {
        None
    }
}
pub(super) fn issue(
    conditions: &Value,
    preparation: &Value,
) -> Option<(&'static str, &'static str, &'static str, bool)> {
    // A preparation trace cannot fill in absent current screen observations.
    if !facts_complete(conditions) {
        return None;
    }
    if state(conditions) == Some("secure_locked") {
        return Some((
            "device_secure_locked",
            "手机处于安全锁定，等待本人解锁",
            "请本人解锁手机",
            false,
        ));
    }
    if state(conditions).is_some() && !preparation["automatic_wake_enabled"].is_boolean() {
        return Some((
            "screen_preparation_unknown",
            "手机屏幕未就绪，等待上报自动准备状态",
            "等待手机更新屏幕准备状态",
            true,
        ));
    }
    if state(conditions).is_some() && preparation["automatic_wake_enabled"] == false {
        return Some((
            "automatic_wake_disabled",
            "手机屏幕未就绪，未开启自动唤醒",
            "亮屏解锁；专用手机可在本机同意按需唤醒",
            false,
        ));
    }
    let failure = match preparation["last_error"].as_str() {
        Some("wake_timeout") => Some(("wake_timeout", "手机亮屏准备超时，系统将按退避重试")),
        Some("keyguard_timeout") => Some(("keyguard_timeout", "手机已亮屏，非安全键盘锁解除超时")),
        Some("keyguard_dismiss_failed") => {
            Some(("keyguard_dismiss_failed", "系统未完成非安全键盘锁解除"))
        }
        Some("keyguard_dismiss_cancelled") => {
            Some(("keyguard_dismiss_cancelled", "非安全键盘锁解除已取消"))
        }
        Some("wake_activity_destroyed") => Some((
            "wake_activity_destroyed",
            "手机亮屏准备页提前关闭，等待重试",
        )),
        Some("open_not_confirmed") => {
            Some(("open_not_confirmed", "尚未在时限内确认企业微信进入前台"))
        }
        _ => None,
    };
    if let Some((code, message)) = failure {
        return Some((
            code,
            message,
            "等待手机重新准备；可在手机查看准备诊断",
            true,
        ));
    }
    let preparing = match preparation["stage"].as_str() {
        Some("preparing_wake") => Some(("preparing_wake", "手机正在准备亮屏")),
        Some("preparing_keyguard") => {
            Some(("preparing_keyguard", "手机已亮屏，正在解除非安全键盘锁"))
        }
        Some("opening_application") => {
            Some(("opening_application", "手机正在打开并确认企业微信前台"))
        }
        _ => None,
    };
    if let Some((code, message)) = preparing {
        return Some((code, message, "等待手机完成当前准备阶段", true));
    }
    if preparation["last_error"] == "wake_launch_denied" {
        return Some((
            "wake_launch_denied",
            "系统未允许后台亮屏，等待手机准备重试",
            "检查手机后台运行限制或手动打开手机助手",
            true,
        ));
    }
    state(conditions)?;
    if state(conditions) == Some("screen_off") {
        Some((
            "device_screen_off",
            "手机屏幕已关闭，有有效待办时自动准备亮屏",
            "系统按实际待办和本机同意准备手机",
            true,
        ))
    } else {
        Some((
            "device_keyguard_locked",
            "手机处于非安全键盘锁，有有效待办时自动准备",
            "系统准备亮屏并请求解除非安全键盘锁",
            true,
        ))
    }
}
pub(super) fn runtime_issue(reason: &Value) -> Option<(&'static str, &'static str)> {
    match reason.as_str()? {
        "recovery_pending" => Some(("recovery_pending", "手机正在恢复未决执行记录，等待完成核对")),
        "authorization_dialog_open" => Some((
            "authorization_dialog_open",
            "手机当前存在本机确认对话框，等待处理",
        )),
        "runtime_paused" | "platform_pause" => {
            Some(("runtime_paused", "手机执行已暂停，托管同意保留"))
        }
        _ => None,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn incomplete_screen_facts_never_infer_wake_or_failure_from_old_preparation() {
        let full = json!({"interactive":false,"keyguard_locked":true,"device_locked":false,"keyguard_secure":false});
        for missing in [
            "interactive",
            "keyguard_locked",
            "device_locked",
            "keyguard_secure",
        ] {
            let mut partial = full.clone();
            partial.as_object_mut().unwrap().remove(missing);
            assert!(!facts_complete(&partial));
            assert_eq!(state(&partial), None);
            for preparation in [
                json!({"automatic_wake_enabled":true}),
                json!({"automatic_wake_enabled":true,"last_error":"wake_timeout"}),
            ] {
                assert!(issue(&partial, &preparation).is_none());
            }
        }
    }
    #[test]
    fn wake_timeout_is_not_launch_denial_and_preparation_is_not_busy() {
        let sleeping = serde_json::json!({"interactive":false,"keyguard_locked":true,"device_locked":false,"keyguard_secure":false});
        for code in [
            "wake_timeout",
            "keyguard_timeout",
            "open_not_confirmed",
            "wake_activity_destroyed",
        ] {
            assert_eq!(
                issue(
                    &sleeping,
                    &serde_json::json!({"automatic_wake_enabled":true,"last_error":code})
                )
                .unwrap()
                .0,
                code
            );
        }
        assert_eq!(
            issue(
                &sleeping,
                &serde_json::json!({"automatic_wake_enabled":true,"stage":"preparing_wake"})
            )
            .unwrap()
            .0,
            "preparing_wake"
        );
    }
    #[test]
    fn distinguishes_sleep_secure_lock_and_denied_wake() {
        let sleeping = json!({"interactive":false,"keyguard_locked":true,"device_locked":false,"keyguard_secure":false});
        assert_eq!(state(&sleeping), Some("screen_off"));
        assert_eq!(
            issue(&sleeping, &json!({"automatic_wake_enabled":true}))
                .unwrap()
                .0,
            "device_screen_off"
        );
        assert_eq!(
            issue(&sleeping, &json!({"automatic_wake_enabled":false}))
                .unwrap()
                .0,
            "automatic_wake_disabled"
        );
        assert_eq!(
            issue(
                &sleeping,
                &json!({"automatic_wake_enabled":true,"last_error":"wake_launch_denied"})
            )
            .unwrap()
            .0,
            "wake_launch_denied"
        );
        let secure = json!({"interactive":false,"keyguard_locked":true,"device_locked":true,"keyguard_secure":true});
        let issue = issue(&secure, &json!({"automatic_wake_enabled":true})).unwrap();
        assert_eq!(issue.0, "device_secure_locked");
        assert!(!issue.3);
        assert_eq!(
            state(&json!({"interactive":true,"keyguard_locked":false,"device_locked":false})),
            None
        );
    }
}
