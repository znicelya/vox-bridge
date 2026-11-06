/** Native display names; unknown two-letter backend codes remain explicit. */
export function languageLabel(code: string): string {
  return (
    (
      {
        en: "English",
        zh: "中文",
        ja: "日本語",
        ko: "한국어",
        fr: "Français",
        de: "Deutsch",
        es: "Español",
        ru: "Русский",
        pt: "Português",
        it: "Italiano",
        ar: "العربية",
        hi: "हिन्दी",
      } as Record<string, string>
    )[code] ?? code
  );
}
