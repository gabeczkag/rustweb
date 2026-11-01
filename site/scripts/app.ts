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

document.addEventListener("rweb:rendered", () => {
  updateYear();
  bindTheme();
});

applyTheme(initialTheme());
updateYear();
bindTheme();
