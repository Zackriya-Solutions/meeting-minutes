import i18n from 'i18next'
import { initReactI18next } from 'react-i18next'

// Import translation files
import commonPt from '../../public/locales/pt/common.json'
import commonEn from '../../public/locales/en/common.json'

const resources = {
  pt: {
    common: commonPt,
  },
  en: {
    common: commonEn,
  },
}

i18n
  .use(initReactI18next)
  .init({
    resources,
    lng: 'pt', // default language
    fallbackLng: 'pt',
    defaultNS: 'common',
    interpolation: {
      escapeValue: false, // React already does escaping
    },
  })

export default i18n
