.pragma library

// Build the exposed boundary of occupied rectangles. Filled material remains on
// the right of every directed edge, so desktop openings have negative area.
function geometry(width, height, rectangles, radius, handles) {
    const limit = value => Math.round(value * 1000) / 1000;
    const boxes = rectangles.map(rect => {
        const x = limit(Math.max(0, Math.min(width, rect.x)));
        const y = limit(Math.max(0, Math.min(height, rect.y)));
        const right = limit(Math.max(0, Math.min(width, rect.x + rect.width)));
        const bottom = limit(Math.max(0, Math.min(height, rect.y + rect.height)));
        return {
            x,
            y,
            right,
            bottom
        };
    }).filter(rect => rect.right > rect.x && rect.bottom > rect.y);
    if (!boxes.length || width <= 0 || height <= 0)
        return {
            body: "",
            rim: "",
            loops: []
        };
    const xs = [...new Set([0, width, ...boxes.reduce((all, r) => all.concat([r.x, r.right]), [])])].sort((a, b) => a - b);
    const ys = [...new Set([0, height, ...boxes.reduce((all, r) => all.concat([r.y, r.bottom]), [])])].sort((a, b) => a - b);
    const occupied = ys.slice(1).map((bottom, row) => xs.slice(1).map((right, col) => {
            const x = (xs[col] + right) / 2, y = (ys[row] + bottom) / 2;
            return boxes.some(r => x > r.x && x < r.right && y > r.y && y < r.bottom);
        }));
    const edges = [], outgoing = {};
    const key = p => `${p.x},${p.y}`;
    function add(a, b, direction) {
        const edge = {
            a,
            b,
            direction,
            used: false
        };
        edges.push(edge);
        if (!outgoing[key(a)])
            outgoing[key(a)] = [];
        outgoing[key(a)].push(edge);
    }
    for (let row = 0; row < occupied.length; row++) {
        for (let col = 0; col < occupied[row].length; col++) {
            if (!occupied[row][col])
                continue;
            const tl = {
                x: xs[col],
                y: ys[row]
            }, tr = {
                x: xs[col + 1],
                y: ys[row]
            };
            const br = {
                x: xs[col + 1],
                y: ys[row + 1]
            }, bl = {
                x: xs[col],
                y: ys[row + 1]
            };
            if (!occupied[row - 1]?.[col])
                add(tl, tr, 0);
            if (!occupied[row]?.[col + 1])
                add(tr, br, 1);
            if (!occupied[row + 1]?.[col])
                add(br, bl, 2);
            if (!occupied[row]?.[col - 1])
                add(bl, tl, 3);
        }
    }
    const loops = [];
    for (const first of edges) {
        if (first.used)
            continue;
        let edge = first;
        const points = [];
        do {
            edge.used = true;
            points.push(edge.a);
            if (key(edge.b) === key(first.a))
                break;
            const choices = (outgoing[key(edge.b)] || []).filter(next => !next.used);
            const rank = next => [1, 0, 3, 2].indexOf((next.direction - edge.direction + 4) % 4);
            choices.sort((a, b) => rank(a) - rank(b));
            edge = choices[0];
            if (!edge)
                throw new Error("Unclosed frame contour");
        } while (points.length <= edges.length)
        loops.push(simplify(points));
    }
    const body = loops.map(points => roundedPath(points, true, width, height, radius, handles)).join(" ");
    const rimParts = loops.map(points => {
        if (area(points) < 0)
            return [roundedPath(points, true, width, height, radius, handles)];
        const excluded = points.map((p, i) => outerEdge(p, points[(i + 1) % points.length], width, height));
        const start = excluded.indexOf(true);
        if (start < 0)
            return [roundedPath(points, true, width, height, radius, handles)];
        const chains = [];
        let chain = [];
        for (let step = 1; step <= points.length; step++) {
            const i = (start + step) % points.length;
            if (excluded[i]) {
                if (chain.length > 1)
                    chains.push(chain);
                chain = [];
            } else {
                if (!chain.length)
                    chain.push(points[i]);
                chain.push(points[(i + 1) % points.length]);
            }
        }
        if (chain.length > 1)
            chains.push(chain);
        return chains.map(chain => roundedPath(chain, false, width, height, radius, handles));
    });
    const rim = [].concat.apply([], rimParts).join(" ");
    return {
        body,
        rim,
        loops
    };
}
function area(points) {
    return points.reduce((sum, p, i) => {
        const next = points[(i + 1) % points.length];
        return sum + p.x * next.y - next.x * p.y;
    }, 0) / 2;
}
function simplify(points) {
    return points.filter((p, i) => {
        const before = points[(i + points.length - 1) % points.length], after = points[(i + 1) % points.length];
        return (p.x - before.x) * (after.y - p.y) !== (p.y - before.y) * (after.x - p.x);
    });
}
function outerEdge(a, b, width, height) {
    return a.x === b.x && (a.x === 0 || a.x === width) || a.y === b.y && (a.y === 0 || a.y === height);
}
function roundedPath(points, closed, width, height, radius, handles) {
    if (points.length < 2)
        return "";
    const near = (a, b) => Math.abs(a - b) < .002;
    const corners = points.map((p, i) => {
        if (!closed && (i === 0 || i === points.length - 1))
            return {
                entry: p,
                exit: p
            };
        const before = points[(i + points.length - 1) % points.length], after = points[(i + 1) % points.length];
        let horizontal = radius, vertical = radius;
        const handle = (handles || []).find(h => (near(p.x, h.x) || near(p.x, h.x + h.width)) && (near(p.y, h.y) || near(p.y, h.y + h.height)));
        if (handle) {
            horizontal = handle.edge === "top" ? handle.shoulder : handle.width / 2;
            vertical = handle.edge === "top" ? handle.height / 2 : handle.shoulder;
        } else if (p.x === 0 || p.x === width || p.y === 0 || p.y === height) {
            horizontal = vertical = 0;
        }
        function toward(other) {
            const dx = other.x - p.x, dy = other.y - p.y;
            const length = Math.abs(dx) + Math.abs(dy);
            const cut = Math.min(dx ? horizontal : vertical, length / 2);
            return {
                x: p.x + Math.sign(dx) * cut,
                y: p.y + Math.sign(dy) * cut
            };
        }
        return {
            entry: toward(before),
            exit: toward(after)
        };
    });
    const number = value => Math.round(value * 1000) / 1000;
    const pair = p => `${number(p.x)} ${number(p.y)}`;
    let path = "M" + pair(corners[0].entry);
    for (let i = 0; i < points.length; i++) {
        const corner = corners[i];
        if (i)
            path += " L" + pair(corner.entry);
        path += " Q" + pair(points[i]) + " " + pair(corner.exit);
    }
    return path + (closed ? " Z" : "");
}
