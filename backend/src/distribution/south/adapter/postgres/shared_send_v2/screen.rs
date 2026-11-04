use serde_json::Value;

pub(super) fn state(conditions: &Value) -> Option<&'static str> {
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
    if state(conditions) == Some("secure_locked") {
        return Some((
            "device_secure_locked",
            "手机处于安全锁定，等待本人解锁",
            "请本人解锁手机",
            false,
        ));
    }
    state(conditions)?;
    if !preparation["automatic_wake_enabled"].is_boolean() {
        return Some((
            "screen_preparation_unknown",
            "手机屏幕未就绪，等待上报自动准备状态",
            "等待手机更新屏幕准备状态",
            true,
        ));
    }
    if preparation["automatic_wake_enabled"] == false {
        return Some((
            "automatic_wake_disabled",
            "手机屏幕未就绪，未开启自动唤醒",
            "亮屏解锁；专用手机可在本机同意按需唤醒",
            false,
        ));
    }
    if preparation["last_error"] == "wake_launch_denied" {
        return Some((
            "wake_launch_denied",
            "系统未允许后台亮屏，等待手机准备重试",
            "检查手机后台运行限制或手动打开手机助手",
            true,
        ));
    }
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
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
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
        let secure = json!({"interactive":false,"device_locked":true,"keyguard_secure":true});
        let issue = issue(&secure, &json!({"automatic_wake_enabled":true})).unwrap();
        assert_eq!(issue.0, "device_secure_locked");
        assert!(!issue.3);
        assert_eq!(
            state(&json!({"interactive":true,"keyguard_locked":false,"device_locked":false})),
            None
        );
    }
}
