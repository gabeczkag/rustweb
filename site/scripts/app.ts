const updateYear = (): void => {
  const year = document.getElementById("year");
  if (year) {
    year.textContent = String(new Date().getFullYear());
  }
};

document.addEventListener("rweb:rendered", updateYear);
updateYear();
