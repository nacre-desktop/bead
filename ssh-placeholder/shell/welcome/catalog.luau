-- SPDX-License-Identifier: GPL-3.0-only
-- Welcome catalog (B0059): the tour's pages, app tiles, setup guides, help text and links as data.
-- Text is message ids. `route` and link names are ids the adapter (`welcome.rs`) resolves against its
-- own fixed table; `needs` names the store that says whether this desktop can open the app.
return {
    steps={"welcome","colors","shortcuts","apps","ready"},
    sections={
        welcome="bead.welcome.section-welcome",colors="bead.welcome.section-colors",
        shortcuts="bead.welcome.section-shortcuts",apps="bead.welcome.section-apps",
        ready="bead.welcome.section-ready",help="bead.welcome.section-help",
    },
    apps={
        {id="settings",icon="settings",route="settings",needs="welcome.can.settings",
            title="bead.welcome.app-settings-title",detail="bead.welcome.app-settings-detail"},
        {id="colors",icon="palette",route="colors",needs="welcome.can.colors",
            title="bead.welcome.app-colors-title",detail="bead.welcome.app-colors-detail"},
        {id="ai",icon="chat_bubble",route="ai",needs="welcome.can.ai",optional=true,
            title="bead.welcome.app-ai-title",detail="bead.welcome.app-ai-detail"},
        {id="brain",icon="neurology",route="brain",needs="welcome.can.brain",optional=true,
            title="bead.welcome.app-brain-title",detail="bead.welcome.app-brain-detail"},
    },
    guides={
        ai={title="bead.welcome.guide-ai-title",detail="bead.welcome.guide-ai-detail",
            setup="bead.welcome.guide-ai-setup",status="bead.welcome.guide-ai-status"},
        brain={title="bead.welcome.guide-brain-title",detail="bead.welcome.guide-brain-detail",
            setup="bead.welcome.guide-brain-setup",status="bead.welcome.guide-brain-status"},
    },
    roles={
        {role="primary",label="bead.welcome.role-main"},{role="secondary",label="bead.welcome.role-second"},
        {role="tertiary",label="bead.welcome.role-third"},{role="surface",label="bead.welcome.role-background"},
    },
    roleLabels={primary="bead.welcome.role-main",secondary="bead.welcome.role-second",
        tertiary="bead.welcome.role-third",surface="bead.welcome.role-background"},
    help={
        {title="bead.welcome.help-panel-title",detail="bead.welcome.help-panel-detail"},
        {title="bead.welcome.help-attention-title",detail="bead.welcome.help-attention-detail"},
        {title="bead.welcome.help-comfort-title",detail="bead.welcome.help-comfort-detail"},
        {title="bead.welcome.help-terms-title",detail="bead.welcome.help-terms-detail"},
    },
    helpRoutes={
        {label="bead.welcome.help-maintenance",route="settings:system"},
        {label="bead.welcome.help-displays",route="settings:displays"},
    },
    links={
        {label="bead.welcome.link-project",link="project"},{label="bead.welcome.link-hyprland",link="hyprland"},
        {label="bead.welcome.link-niri",link="niri"},{label="bead.welcome.link-cachyos",link="cachyos"},
    },
}
