# Agent Routing And Handoff

Xero route suggestions use a single assistant marker:

```xml
<xero-routing-suggestion target="engineer" reason="short rationale" summary="one-sentence carry-over summary"/>
```

Custom-agent targets use `targetKind="custom"` and a definition id:

```xml
<xero-routing-suggestion targetKind="custom" definitionId="release_helper" reason="short rationale" summary="one-sentence carry-over summary"/>
```

Built-in route targets are limited to `ask`, `plan`, `engineer`, `debug`, and `generalist`. `computer_use`, `crawl`, and `agent_create` are excluded from route suggestions and cross-agent handoff.

Plan is intentionally narrow: it may route or hand off only to built-in Engineer. It must not target Ask, Debug, Generalist, custom agents, Computer Use, Crawl, or Agent Create.

Custom agents store the current handoff contract in `handoffPolicy`:

```json
{
  "enabled": true,
  "routingMode": "same_agent",
  "allowedTargets": [],
  "preserveDefinitionVersion": true,
  "carrySummary": true,
  "includeDurableContext": true
}
```

`routingMode: "same_agent"` allows continuation under the same pinned agent definition only. `routingMode: "suggest"` allows cross-agent suggestions, but only to explicit `allowedTargets`.

Allowed target refs are typed:

```json
{ "kind": "built_in", "runtimeAgentId": "engineer" }
{ "kind": "custom", "definitionId": "release_helper", "version": 3 }
```

Custom-agent built-in targets may include Ask, Engineer, Debug, or Generalist. Plan and the excluded runtime agents are rejected as configurable custom-agent targets.
