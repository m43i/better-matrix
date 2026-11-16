// Translations: the desktop's catalogs (apps/desktop/i18n/app) with the mobile catalog layered
// on top, so both apps say the same thing in English, German and French (ADR 0010). Keys and
// `{{name}}` placeholders follow the desktop's format; plurals use `.one` / `.other` keys.

import { getLocales } from 'expo-localization';

import desktopDe from '../../../desktop/i18n/app/de.json';
import desktopEn from '../../../desktop/i18n/app/en.json';
import desktopFr from '../../../desktop/i18n/app/fr.json';
import shellEn from './mobile/shell.en.json';
import shellDe from './mobile/shell.de.json';
import shellFr from './mobile/shell.fr.json';
import conversationEn from './mobile/conversation.en.json';
import conversationDe from './mobile/conversation.de.json';
import conversationFr from './mobile/conversation.fr.json';
import composerEn from './mobile/composer.en.json';
import composerDe from './mobile/composer.de.json';
import composerFr from './mobile/composer.fr.json';
import accountsEn from './mobile/accounts.en.json';
import accountsDe from './mobile/accounts.de.json';
import accountsFr from './mobile/accounts.fr.json';
import peopleEn from './mobile/people.en.json';
import peopleDe from './mobile/people.de.json';
import peopleFr from './mobile/people.fr.json';
import callsEn from './mobile/calls.en.json';
import callsDe from './mobile/calls.de.json';
import callsFr from './mobile/calls.fr.json';
import pushEn from './mobile/push.en.json';
import pushDe from './mobile/push.de.json';
import pushFr from './mobile/push.fr.json';
import linksEn from './mobile/links.en.json';
import linksDe from './mobile/links.de.json';
import linksFr from './mobile/links.fr.json';

export type Locale = 'en' | 'de' | 'fr';
/** The app's language: the system's, or one catalog. */
export type Language = 'system' | Locale;

export const LOCALES: ReadonlyArray<{ id: Locale; label: string }> = [
  { id: 'en', label: 'English' },
  { id: 'de', label: 'Deutsch' },
  { id: 'fr', label: 'Français' },
];

type Catalog = Record<string, string>;

/** Every catalog, by locale: the desktop's, then the mobile's over it. */
// One mobile catalog per area of the app (src/i18n/mobile/<area>.<locale>.json), so work on
// one area never edits another's file.
const mobileEn = { ...shellEn, ...conversationEn, ...composerEn, ...accountsEn, ...peopleEn, ...callsEn, ...linksEn, ...pushEn };
const mobileDe = { ...shellDe, ...conversationDe, ...composerDe, ...accountsDe, ...peopleDe, ...callsDe, ...linksDe, ...pushDe };
const mobileFr = { ...shellFr, ...conversationFr, ...composerFr, ...accountsFr, ...peopleFr, ...callsFr, ...linksFr, ...pushFr };

export const catalogs: Record<Locale, Catalog> = {
  en: { ...desktopEn, ...mobileEn },
  de: { ...desktopDe, ...mobileDe },
  fr: { ...desktopFr, ...mobileFr },
};

/** The mobile catalogs alone, by locale: the tests check German and French cover English. */
export const mobileCatalogs: Record<Locale, Catalog> = { en: mobileEn, de: mobileDe, fr: mobileFr };

/** A key every catalog has: English is the source. */
export type Key = keyof typeof desktopEn | keyof typeof mobileEn;

let current: Locale = 'en';

/** The locale `language` resolves to on this device. */
export function resolve(language: Language): Locale {
  if (language !== 'system') return language;
  for (const locale of getLocales()) {
    const code = locale.languageCode as Locale | null;
    if (code && code in catalogs) return code;
  }
  return 'en';
}

/** Switches the language for every later lookup. */
export function setLanguage(language: Language): void {
  current = resolve(language);
}

/** The locale in use. */
export function locale(): Locale {
  return current;
}

/** The message for `key`, with `{{name}}` placeholders filled. English fills any gap. */
export function t(key: Key, params?: Record<string, string | number>): string {
  const message = catalogs[current][key] ?? catalogs.en[key] ?? key;
  if (!params) return message;
  return message.replace(/\{\{(\w+)\}\}/g, (whole, name: string) =>
    name in params ? format(params[name]!) : whole,
  );
}

/** The plural form of `key` for `count` (`key.one` or `key.other`), with `{{count}}` filled. */
export function tp(key: string, count: number, params?: Record<string, string | number>): string {
  const form = pluralForm(current, count);
  return t(`${key}.${form}` as Key, { count, ...params });
}

/**
 * CLDR's cardinal plural form for our catalogs' languages. Hermes on Android has no
 * `Intl.PluralRules`, so the rules are spelled out: English and German say "one" for exactly 1,
 * French for 0 and 1 (and the fractions between).
 */
export function pluralForm(locale: Locale, count: number): 'one' | 'other' {
  if (locale === 'fr') return count >= 0 && count < 2 ? 'one' : 'other';
  return count === 1 ? 'one' : 'other';
}

function format(value: string | number): string {
  return typeof value === 'number' ? value.toLocaleString(current) : value;
}
