use super::{NotificationAdapterError, NotificationRouteKind};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedNotificationRouteTarget {
    pub route_kind: NotificationRouteKind,
    pub channel_target: String,
}

impl ParsedNotificationRouteTarget {
    pub fn canonical(&self) -> String {
        format!("{}:{}", self.route_kind.as_str(), self.channel_target)
    }
}

pub fn parse_notification_route_target(
    raw_target: &str,
) -> Result<ParsedNotificationRouteTarget, NotificationAdapterError> {
    let trimmed = raw_target.trim();
    let Some((prefix, channel_target)) = trimmed.split_once(':') else {
        return Err(NotificationAdapterError::payload_invalid(format!(
            "Cadence requires route targets in `<kind>:<channel-target>` format. Received `{trimmed}`."
        )));
    };

    let route_kind = NotificationRouteKind::parse(prefix.trim())?;
    let channel_target = channel_target.trim();
    if channel_target.is_empty() {
        return Err(NotificationAdapterError::payload_invalid(format!(
            "Cadence requires route targets in `<kind>:<channel-target>` format for `{}` notifications.",
            route_kind.as_str()
        )));
    }

    Ok(ParsedNotificationRouteTarget {
        route_kind,
        channel_target: channel_target.to_string(),
    })
}

pub fn parse_notification_route_target_for_kind(
    route_kind: NotificationRouteKind,
    raw_target: &str,
) -> Result<ParsedNotificationRouteTarget, NotificationAdapterError> {
    let trimmed = raw_target.trim();
    let Some((prefix, channel_target)) = trimmed.split_once(':') else {
        return Err(NotificationAdapterError::payload_invalid(format!(
            "Cadence requires route targets in `<kind>:<channel-target>` format for `{}` notifications.",
            route_kind.as_str()
        )));
    };

    let prefix = prefix.trim();
    let channel_target = channel_target.trim();

    if prefix != route_kind.as_str() || channel_target.is_empty() {
        return Err(NotificationAdapterError::payload_invalid(format!(
            "Cadence requires route targets in `<kind>:<channel-target>` format for `{}` notifications.",
            route_kind.as_str()
        )));
    }

    Ok(ParsedNotificationRouteTarget {
        route_kind,
        channel_target: channel_target.to_string(),
    })
}

pub fn compose_notification_route_target(
    route_kind: NotificationRouteKind,
    raw_target: &str,
) -> Result<String, NotificationAdapterError> {
    let trimmed = raw_target.trim();
    if trimmed.is_empty() {
        return Err(NotificationAdapterError::payload_invalid(format!(
            "Cadence requires a non-empty route target for `{}` notifications.",
            route_kind.as_str()
        )));
    }

    if let Some((prefix, channel_target)) = trimmed.split_once(':') {
        let prefix = prefix.trim();
        let channel_target = channel_target.trim();

        if prefix != route_kind.as_str() || channel_target.is_empty() {
            return Err(NotificationAdapterError::payload_invalid(format!(
                "Cadence requires route targets in `<kind>:<channel-target>` format for `{}` notifications.",
                route_kind.as_str()
            )));
        }

        return Ok(format!("{}:{}", route_kind.as_str(), channel_target));
    }

    Ok(format!("{}:{}", route_kind.as_str(), trimmed))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_for_kind_rejects_missing_prefix_and_mismatch() {
        let missing_prefix = parse_notification_route_target_for_kind(
            NotificationRouteKind::Discord,
            "123456789012345678",
        )
        .expect_err("missing prefix must fail closed");
        assert_eq!(missing_prefix.code, "notification_adapter_payload_invalid");

        let mismatch = parse_notification_route_target_for_kind(
            NotificationRouteKind::Discord,
            "telegram:@ops",
        )
        .expect_err("mismatched prefix must fail closed");
        assert_eq!(mismatch.code, "notification_adapter_payload_invalid");

        let empty_channel =
            parse_notification_route_target_for_kind(NotificationRouteKind::Discord, "discord:   ")
                .expect_err("empty channel must fail closed");
        assert_eq!(empty_channel.code, "notification_adapter_payload_invalid");
    }

    #[test]
    fn parse_for_kind_canonicalizes_channel_whitespace() {
        let parsed = parse_notification_route_target_for_kind(
            NotificationRouteKind::Telegram,
            " telegram :  @ops-room  ",
        )
        .expect("canonical target should parse");

        assert_eq!(parsed.channel_target, "@ops-room");
        assert_eq!(parsed.canonical(), "telegram:@ops-room");
    }

    #[test]
    fn compose_is_idempotent_for_plain_and_canonical_inputs() {
        let composed_plain =
            compose_notification_route_target(NotificationRouteKind::Discord, "123456789012345678")
                .expect("plain target should compose");
        assert_eq!(composed_plain, "discord:123456789012345678");

        let composed_canonical = compose_notification_route_target(
            NotificationRouteKind::Discord,
            " discord:123456789012345678 ",
        )
        .expect("canonical target should remain canonical");
        assert_eq!(composed_canonical, "discord:123456789012345678");

        let mismatch = compose_notification_route_target(
            NotificationRouteKind::Telegram,
            "discord:123456789012345678",
        )
        .expect_err("mismatched canonical target must fail");
        assert_eq!(mismatch.code, "notification_adapter_payload_invalid");
    }

    #[test]
    fn parse_any_kind_rejects_empty_channel() {
        let error = parse_notification_route_target("telegram:")
            .expect_err("empty channel target should fail closed");
        assert_eq!(error.code, "notification_adapter_payload_invalid");
    }
}
