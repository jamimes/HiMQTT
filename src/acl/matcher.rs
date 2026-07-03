use rumqttd::protocol;

/// 判断订阅 filter 是否被 ACL pattern 覆盖（Mosquitto 风格 read 规则）
pub fn subscribe_allowed(acl_pattern: &str, sub_filter: &str) -> bool {
    if acl_pattern == sub_filter {
        return true;
    }
    if let Some(prefix) = acl_pattern.strip_suffix("/#") {
        return sub_filter.starts_with(&format!("{prefix}/")) || sub_filter == prefix;
    }
    if let Some(prefix) = acl_pattern.strip_suffix('#') {
        return sub_filter.starts_with(prefix);
    }
    false
}

/// 判断 publish topic 是否匹配 ACL pattern
pub fn publish_allowed(acl_pattern: &str, topic: &str) -> bool {
    protocol::matches(topic, acl_pattern)
}

/// 判断 topic/filter 是否匹配已注册 Topic 条目
pub fn registered_covers(registered: &str, target: &str) -> bool {
    if registered == target {
        return true;
    }
    if registered.ends_with('#') {
        let prefix = registered.trim_end_matches('#');
        return target.starts_with(prefix);
    }
    if let Some(prefix) = registered.strip_suffix("/#") {
        return target.starts_with(&format!("{prefix}/")) || target == prefix;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subscribe_prefix_wildcard() {
        assert!(subscribe_allowed("home/#", "home/room/temp"));
        assert!(!subscribe_allowed("home/#", "office/room"));
    }

    #[test]
    fn publish_wildcard() {
        assert!(publish_allowed("home/+/temp", "home/a/temp"));
        assert!(!publish_allowed("home/+/temp", "home/a/humid"));
    }
}
