const THEME_KEY = "rustweb.theme";

type Theme = "light" | "dark";

const updateYear = (): void => {
  const year = document.getElementById("year");
  if (year) {
    year.textContent = String(new Date().getFullYear());
  }
};

const themeButton = (): HTMLButtonElement | null =>
  document.getElementById("theme-toggle") as HTMLButtonElement | null;

const currentTheme = (): Theme =>
  document.documentElement.dataset.theme === "light" ? "light" : "dark";

const applyTheme = (theme: Theme): void => {
  document.documentElement.dataset.theme = theme;
  const button = themeButton();
  if (button) {
    button.textContent = theme === "light" ? "CIEMNY" : "JASNY";
    button.setAttribute("aria-pressed", String(theme === "light"));
  }
};

const savedTheme = (): Theme | null => {
  try {
    const value = localStorage.getItem(THEME_KEY);
    return value === "light" || value === "dark" ? value : null;
  } catch {
    return null;
  }
};

const initialTheme = (): Theme => {
  const query = new URLSearchParams(location.search).get("theme");
  if (query === "light" || query === "dark") return query;
  const saved = savedTheme();
  if (saved) return saved;
  return window.matchMedia?.("(prefers-color-scheme: light)").matches
    ? "light"
    : "dark";
};

const bindTheme = (): void => {
  applyTheme(currentTheme());
  const button = themeButton();
  if (!button || button.dataset.bound === "1") return;

  button.dataset.bound = "1";
  button.addEventListener("click", () => {
    const next = currentTheme() === "light" ? "dark" : "light";
    applyTheme(next);
    try {
      localStorage.setItem(THEME_KEY, next);
    } catch {}
  });
};

const copyText = async (text: string): Promise<boolean> => {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    const area = document.createElement("textarea");
    area.value = text;
    area.style.position = "fixed";
    area.style.opacity = "0";
    document.body.append(area);
    area.select();
    const copied = document.execCommand("copy");
    area.remove();
    return copied;
  }
};

const bindCopyButtons = (): void => {
  document.querySelectorAll<HTMLButtonElement>("[data-copy]").forEach((button) => {
    if (button.dataset.bound === "1") return;
    button.dataset.bound = "1";

    button.addEventListener("click", async () => {
      const selector = button.dataset.copy || "";
      const target = selector ? document.querySelector(selector) : null;
      const code = target || button.closest(".code-block")?.querySelector("code");
      const text = code?.textContent || "";
      if (!text) return;

      const original = button.textContent || "KOPIUJ";
      if (await copyText(text)) {
        button.textContent = "SKOPIOWANO";
        window.setTimeout(() => {
          button.textContent = original;
        }, 1400);
      }
    });
  });
};

document.addEventListener("rweb:rendered", () => {
  updateYear();
  bindTheme();
  bindCopyButtons();
});

applyTheme(initialTheme());
updateYear();
bindTheme();
bindCopyButtons();
