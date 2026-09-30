import { Fragment, useState } from "react";
import type { KeyboardEvent } from "react";

const __tabs = [

    { id: "home", label: "홈", icon: "home-door" },
    { id: "tabs", label: "탭", icon: "square-behind-square-4" },
    { id: "history", label: "기록", icon: "clock" },
    { id: "connections", label: "연결", icon: "circles-three" },
    { id: "more", label: "더 보기", icon: "dot-grid-1x3-horizontal-tight" },
    { id: "locations", label: "위치", icon: "pin-location" },
    { id: "branches", label: "브랜치", icon: "branch-simple" },
    { id: "packages", label: "패키지", icon: "3d-box-top" },
    { id: "files", label: "파일", icon: "folder-2" },
    { id: "profile", label: "프로필", icon: "user" },

] as const;

type tab_id = (typeof __tabs)[number]["id"];

function __Icon({ name, filled = false }: { name: string; filled?: boolean }) {

    const source = `/central-icons/${filled ? "fill" : "reversed"}/${name}.svg`;

    return (

        <span
            className   = "tab-icon"
            aria-hidden = "true"
            style       = {{ maskImage: `url("${source}")`, WebkitMaskImage: `url("${source}")` }}
        />

    );

}

/** 사용자 표시 영역을 비워 두고 창 chrome과 세로 tab 선택을 제공해. */
export function __AppShell() {

    const [active_tab, set_active_tab] = useState<tab_id>("home");

    function navigate_tabs(event: KeyboardEvent<HTMLButtonElement>, current_id: tab_id) {

        const index = __tabs.findIndex((tab) => tab.id === current_id);
        let next_index = index;

        if (event.key === "ArrowDown") next_index = (index + 1) % __tabs.length;
        else if (event.key === "ArrowUp") next_index = (index - 1 + __tabs.length) % __tabs.length;
        else if (event.key === "Home") next_index = 0;
        else if (event.key === "End") next_index = __tabs.length - 1;
        else return;

        event.preventDefault();
        const next_tab = __tabs[next_index];

        if (next_tab) {

            set_active_tab(next_tab.id);
            event.currentTarget.closest("[role='tablist']")
                ?.querySelector<HTMLButtonElement>(`#tab-${next_tab.id}`)?.focus();

        }

    }

    return (

        <div className="app-shell">
            <header className="titlebar">
                <div className="window-controls" role="img" aria-label="macOS 창 닫기, 최소화, 전체 화면">
                    <span className="window-control is-close" />
                    <span className="window-control is-minimize" />
                    <span className="window-control is-fullscreen" />
                </div>
                <div className="topbar" data-tauri-drag-region />
            </header>
            <div className="workspace">
                <nav className="tab-rail" role="tablist" aria-label="탭" aria-orientation="vertical">
                    {__tabs.map((tab, index) => (

                        <Fragment key={tab.id}>
                            {index === 5 && <div className="rail-divider" role="presentation" />}
                            <button
                                id            = {`tab-${tab.id}`}
                                className     = {`rail-tab${tab.id === "profile" ? " profile-tab" : ""}`}
                                type          = "button"
                                role          = "tab"
                                aria-label    = {tab.label}
                                aria-selected = {active_tab === tab.id}
                                aria-controls = "tab-panel"
                                title         = {tab.label}
                                tabIndex      = {active_tab === tab.id ? 0 : -1}
                                onClick       = {() => set_active_tab(tab.id)}
                                onKeyDown     = {(event) => navigate_tabs(event, tab.id)}
                            >
                                <__Icon name={tab.icon} filled={tab.id === "home"} />
                                {active_tab === tab.id && tab.id !== "profile" && (

                                    <span className="active-dot" aria-hidden="true" />

                                )}
                            </button>
                        </Fragment>

                    ))}
                </nav>
                <main
                    id              = "tab-panel"
                    className       = "tab-panel"
                    role            = "tabpanel"
                    aria-labelledby = {`tab-${active_tab}`}
                    tabIndex        = {0}
                />
            </div>
        </div>

    );

}
