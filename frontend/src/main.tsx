import { StrictMode }  from "react";
import { createRoot }  from "react-dom/client";
import { isTauri }     from "@tauri-apps/api/core";
import { __AppShell }  from "./app_shell";
import "./theme.css";

const __root         = document.getElementById("root");
const __native_macos = isTauri() && /Mac/.test(navigator.platform);

document.documentElement.dataset.runtime = __native_macos ? "macos" : "browser";

if (!__root) {

    throw new Error("앱을 표시할 root가 없습니다");

}

createRoot(__root).render(<StrictMode><__AppShell /></StrictMode>);
