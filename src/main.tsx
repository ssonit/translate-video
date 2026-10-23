import React from "react";
import ReactDOM from "react-dom/client";
import "@fontsource/fraunces/600.css";
import "@fontsource/outfit/400.css";
import "@fontsource/outfit/500.css";
import App from "./App";
import ScrollTop from "./ScrollTop";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
    <ScrollTop />
  </React.StrictMode>,
);
