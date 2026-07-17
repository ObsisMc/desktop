import type { ReactNode } from "react";
import { I18nextProvider } from "react-i18next";
import { appI18n } from "./i18n-instance";

export type { Locale, TranslationKey } from "./i18n-instance";

/** Binds the app-specific i18next instance without mutating a host application's global instance. */
export function AppI18nProvider({ children }: { children: ReactNode }) {
  return <I18nextProvider i18n={appI18n}>{children}</I18nextProvider>;
}
