// Exact Shell COPYENGINE HRESULT values; retain the original code in the report.
export function nativeResultReason(hresult: number): string | null {
  switch (hresult >>> 0) {
    case 0x80070070: case 0x80070027: return '대상 저장 장치의 여유 공간이 부족합니다. 공간을 확보한 뒤 원본과 대상의 부분 결과를 확인하세요.';
    case 0x80070005: return '파일이나 폴더에 접근할 권한이 없습니다. 원본과 대상의 접근 권한을 확인하세요.';
    case 0x80070020: case 0x80070021: return '파일을 다른 앱에서 사용 중입니다. 해당 앱에서 파일을 닫은 뒤 결과를 확인하세요.';
    case 0x80270027: return '원본 파일을 다른 앱이 사용 중입니다. 해당 앱에서 파일을 닫은 뒤 다시 시도하세요.';
    case 0x80270028: return '대상 파일을 다른 앱이 사용 중입니다. 해당 앱에서 파일을 닫은 뒤 다시 시도하세요.';
    case 0x80270021: return '원본에 접근할 권한이 없습니다. 원본의 접근 권한을 확인하세요.';
    case 0x80270022: return '대상에 접근할 권한이 없습니다. 대상 폴더의 접근 권한을 확인하세요.';
    case 0x80270023: return '원본 경로를 찾을 수 없습니다. 목록을 새로 고친 뒤 확인하세요.';
    case 0x80270024: return '대상 경로를 찾을 수 없습니다. 대상 폴더를 확인하세요.';
    case 0x80270032: case 0x80270033: return '대상 저장 장치의 여유 공간이 부족합니다. 공간을 확보하고 원본과 대상의 상태를 확인하세요.';
    case 0x8027000d: return '파일 크기가 대상 저장 장치에서 지원하는 한도를 넘었습니다.';
    case 0x8027003f: return '읽기 전용 항목이라 변경하지 못했습니다. 항목의 속성을 확인하세요.';
    default: return null;
  }
}

export function nativeResultCode(hresult: number): string {
  return `0x${(hresult >>> 0).toString(16).padStart(8, '0')}`;
}
