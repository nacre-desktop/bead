# SPDX-License-Identifier: GPL-3.0-only
"""Exact canonical menu.query payload fixtures; no query interpretation or ranking."""
def reply(query, rows, mode="apps", route="apps", revision=1):
    items = {}
    order = []
    fields = ("name", "kind", "description", "breadcrumb", "icon", "emoji",
              "brand", "status", "checked", "reason", "disabled")
    for row in rows:
        ident = row["id"]
        assert ident not in items, "duplicate fixture ID"
        item = {key: row[key] for key in fields if key in row}
        item["activation"] = "fixture:" + ident
        items[ident] = item
        order.append(ident)
    return dict(query=query, mode=mode, route=route, catalog_rev=revision,
                items=items, order=order, total=len(order))
