import { useState } from 'react';
import { setLanguage, t, useLanguage, type Language } from './i18n';
import './language.css';

export function LanguageControl() {
  const language = useLanguage();
  const [saveFailed, setSaveFailed] = useState(false);
  return <div className="language-control">
    <label>{t('언어', 'Language')}
      <select aria-label={t('언어 / Language', 'Language / 언어')} value={language} onChange={event => setSaveFailed(!setLanguage(event.target.value as Language))}>
        <option value="ko" lang="ko">한국어</option><option value="en" lang="en">English</option>
      </select>
    </label>
    {saveFailed && <span className="language-warning" role="alert">{t('언어 설정을 저장하지 못했습니다. 이번 실행에만 적용됩니다.', 'The language preference could not be saved. It applies to this session only.')}</span>}
  </div>;
}
