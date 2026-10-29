.pragma library

// Positions are local to the usable viewport; geometry stays independent of
// object parenting and public panel implementations.
function attached(edge, viewportWidth, viewportHeight, width, height, offset) {
    const centeredX = Math.max(0, (viewportWidth - width) / 2);
    const centeredY = Math.max(0, (viewportHeight - height) / 2);
    if (edge === "left")
        return {
            x: 0,
            y: centeredY
        };
    if (edge === "right")
        return {
            x: Math.max(0, viewportWidth - width - (offset || 0)),
            y: centeredY
        };
    if (edge === "bottom")
        return {
            x: centeredX,
            y: Math.max(0, viewportHeight - height)
        };
    return {
        x: centeredX,
        y: 0
    };
}

function popout(viewportX, viewportWidth, center, width, targetWidth, corner) {
    const relativeCenter = center - viewportX;
    const safeCorner = Math.max(0, Math.min(corner, viewportWidth / 2));
    const targetEnd = relativeCenter + targetWidth / 2;
    const dockRight = targetEnd >= viewportWidth - safeCorner;
    const rightLimit = Math.max(0, viewportWidth - width);
    const centered = relativeCenter - width / 2;
    return {
        x: dockRight ? rightLimit : Math.min(rightLimit, Math.max(safeCorner, centered)),
        y: 0,
        joinsRight: dockRight
    };
}
