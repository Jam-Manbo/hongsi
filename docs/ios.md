# iOS 앱과 위젯

Android와 동일한 Svelte 화면과 `hongsi-direct` 학교 연동 코드를 iOS 앱에서 사용한다. iOS 전용 코드는 Keychain, 위치 권한, 파일 미리보기·공유, WidgetKit, 위젯 링크를 담당한다. 최소 버전은 iOS 17이다.

## 빌드

macOS, Xcode 26 이상과 iOS SDK, Rust, Node/pnpm, XcodeGen 2.46 이상이 필요하다. Xcode의 첫 실행 설정과 Simulator 런타임 설치를 먼저 마친다.

```sh
brew install xcodegen
rustup target add aarch64-apple-ios aarch64-apple-ios-sim
pnpm --dir app install --frozen-lockfile
pnpm --dir app ios:build
```

Intel Mac의 Simulator는 `x86_64-apple-ios` Rust 타깃이 추가로 필요하다. 빌드 스크립트는 현재 Mac의 아키텍처를 사용한다.

시뮬레이터 앱 경로는 `target/ios-xcode/Build/Products/Debug-iphonesimulator/Hongsi.app`이다. 시뮬레이터 빌드도 공유 저장소·Keychain entitlement를 적용하도록 로컬 임시 서명을 한다. `CODE_SIGNING_ALLOWED=NO`로 이 단계를 생략하면 초기 자동 로그인 정보 조회가 실패할 수 있다. 실행할 Simulator를 부팅한 뒤 다음 명령을 사용한다.

```sh
xcrun simctl install booted target/ios-xcode/Build/Products/Debug-iphonesimulator/Hongsi.app
xcrun simctl launch booted dev.kyuyoung.hongsi
```

`app/src-tauri/gen/apple/project.yml`이 Xcode 프로젝트의 원본이다. 빌드 스크립트가 프로젝트를 생성하므로 설정은 생성된 `.xcodeproj` 대신 YAML에 반영한다. 웹 UI 수정 후에는 `ios:build`를 다시 실행한다. 이후에는 생성된 프로젝트를 Xcode에서 열어 실행할 수도 있다.

네이티브 빌드는 `ios/tools/swift`·`xcodebuild` 어댑터로 Cargo가 사용하는 세 개의 Swift 라이브러리를 직접 컴파일한다. Tauri·알림 플러그인 소스는 Cargo 레지스트리, SwiftRs 소스는 라이선스를 포함한 `ios/vendor/swift-rs`를 사용한다. SwiftPM 원격 의존성 해석과 Git 실행은 하지 않는다. 지원하지 않는 Swift 패키지는 자동으로 가져오지 않고 오류로 중단한다. 의존성을 올릴 때는 이 어댑터도 확인해야 한다.

`HONGSI_BUILD_COMMIT`은 빌드 스크립트에서 빈 문자열 또는 CI의 `GITHUB_SHA`로 전달한다. iOS 빌드는 버전 표기를 위해 Git을 실행하지 않는다.

## iPad / iPhone에서 개발 테스트

현재 기본 실행은 배포가 아닌 로컬 기기 테스트다. Xcode의 Run과 `ios:device`는 `Personal` 구성을 사용한다. 무료 Apple 계정에서 지원하지 않는 원격 푸시 entitlement를 제외하며, 앱·위젯 기능과 로컬 알림 코드는 유지한다.

```sh
pnpm --dir app ios:device DEVELOPMENT_TEAM=YOUR_TEAM_ID -allowProvisioningUpdates
```

공유 프로젝트에는 개발팀 ID나 Apple 계정 정보를 저장하지 않는다. 실기기 테스트가 필요할 때만 위 명령의 `YOUR_TEAM_ID`를 자신의 팀 ID로 바꿔 전달한다. Xcode에서 직접 실행할 경우 생성된 로컬 프로젝트의 앱과 위젯 타깃에 같은 팀을 선택하며, 해당 설정을 원본 `project.yml`에 옮기지 않는다. 생성된 Xcode 프로젝트와 사용자 설정은 Git 추적에서 제외된다. 기기를 연결해 신뢰·개발자 모드를 켜고 해당 기기로 Run한다. 실기기에는 테스트용 앱도 개발 인증서와 프로비저닝 프로파일이 필요하다. Xcode 자동 서명이 이를 관리하므로 수동 인증서 발급이나 App Store 등록은 필요하지 않다. 무료 계정으로도 개인 기기 테스트가 가능하다. [Apple 기기 실행 안내](https://help.apple.com/xcode/mac/current/en.lproj/dev5a825a1ca.html)

앱과 위젯이 저장소와 로그인 정보를 함께 사용하므로 다음 설정이 양쪽에 필요하다.

| 항목 | 설정 |
| --- | --- |
| 앱 Bundle ID | `dev.kyuyoung.hongsi` |
| 위젯 Bundle ID | `dev.kyuyoung.hongsi.widgets` |
| App Group | `group.dev.kyuyoung.hongsi` — 양쪽 타깃에서 등록·선택 |
| Keychain Access Group | `$(AppIdentifierPrefix)dev.kyuyoung.hongsi.shared` — 양쪽 동일 |

App Groups는 무료 계정에서도 지원하지만, `-allowProvisioningUpdates`만으로 미등록 App Group 생성·연결이 해결되지 않을 수 있다. `provisioning profile ... doesn't match ... application-groups` 오류가 나오면 Xcode의 Signing & Capabilities에서 양쪽 타깃의 그룹 연결을 확인한다. [Apple 지원표](https://developer.apple.com/help/account/reference/supported-capabilities-ios)

설치 후 Xcode가 `Developer App Certificate is not trusted`를 표시하면 기기의 설정 → 일반 → VPN 및 기기 관리 → 개발자 앱에서 해당 Apple 계정의 인증서를 직접 신뢰한 뒤 다시 Run한다. 앱 설치·서명 성공과 기기에서의 인증서 신뢰 승인은 별도 단계다.

Xcode가 라이선스 동의를 요구하면 사용자가 Xcode를 열어 내용을 확인하고 동의해야 빌드할 수 있다. `sudo xcodebuild -license`로 직접 확인할 수도 있다.

기존 Release/APNs 구성은 남아 있으나 디자인·기능 테스트에 필요하지 않다. 개발 테스트 단계에서 Archive·업로드는 실행하지 않는다.

## 위젯

| 구성 | Android 기본 크기 | iOS 크기 | 기능 |
| --- | --- | --- | --- |
| 빠른 출결 | 4×2 | medium | 출결 상태, 출석번호 입력·제출 |
| 오늘 수업 | 4×2 | medium | 남은 수업·강의실·진행 상태 |
| 오늘 수업 크게 | 4×4 | large | 오늘 수업을 넓게 표시 |
| 다가오는 마감 | 4×2 | medium | 과제·온라인 강의·할 일의 마감 |
| 다가오는 마감 크게 | 4×4 | large | 더 많은 마감 표시 |
| 열람실 좌석 | 2×2 | small | 좌석·남은 시간, 연장·퇴실 확인 화면 이동 |
| 주간 시간표 | 4×4 | large | 요일·교시·강의실, 현재 시각 선 |

위젯 7종은 각 한 크기만 제공한다. 빠른 출결 large는 제거했고, 열람실은 Android와 같은 작은 세로 배치로 변경했다. iOS의 small/medium/large 실제 크기는 기기·홈 화면 설정에 따라 달라지므로 Android의 셀 수와 정확히 같은 픽셀 크기를 뜻하지 않는다.

내부 여백·글자 크기 비율·색상·버튼 배치·마감 테두리·상태 배지·시간표 배치를 Android XML과 렌더러 기준으로 맞췄다. 새로고침·마감 아이콘은 Android vector path를 SVG asset으로 옮겼다. iOS 기본 content margin은 끄고 내부 여백을 직접 적용한다. 글꼴은 각 OS의 시스템 글꼴을 사용하며 외곽 모서리는 WidgetKit을 따른다.

`WidgetPreview.swift`는 Android 미리보기와 같은 과목명·날짜·좌석 예시를 고정 시각으로 제공한다. 스크린샷을 붙이는 대신 실제 위젯 View가 그 데이터를 표시하므로 라이트/다크 테마와 실제 동작 화면의 레이아웃을 공유한다. 주말이나 날짜가 지나도 미리보기가 빈 상태로 바뀌지 않는다.

앱 로그인 후 시간표 등 데이터를 한 번 불러오고 홈 화면의 위젯 추가에서 홍시를 선택한다. 앱이 실행되지 않을 때 학교 데이터를 가져오거나 출석·좌석 연장을 하려면 자동 로그인을 켜야 한다. 위젯 출석 전에 앱에서 위치 권한도 허용한다.

학교 ID/비밀번호/세션은 공유 Keychain에만 저장한다. App Group에는 계정 해시와 표시 데이터·동작 상태를 저장하며, 원본 학번이나 비밀번호를 기록하지 않는다. 로그아웃·계정 전환 시 위젯 상태를 지우고, 대기 중인 요청의 결과가 이전 계정 데이터를 되살리지 않도록 계정을 검증한다. 출석 제출 및 좌석 연장은 중복 실행을 방지하고, 응답이 불확실한 변경 요청을 자동 재시도하지 않는다.

WidgetKit의 백그라운드 갱신은 iOS가 일정을 결정한다. 15분 뒤 갱신을 요청하며, 로컬 시각 변화는 타임라인으로 표시한다. Android처럼 초 단위 네트워크 갱신을 보장하지 않는다. 오래된 출석 가능 상태는 20초 뒤 만료시키고 출석 시작·제출 시 학교 상태를 다시 확인한다. 시간표는 앱에서 동기화한 데이터를 사용한다.

수동 새로고침은 `Toggle(isOn:intent:)`를 버튼 형태로 표시하고, 아이콘의 배경과 프레임 전체를 터치 영역으로 지정한다. 앱을 열지 않고 데이터를 갱신하며, 동작 줄이기가 꺼져 있으면 아이콘을 2초 동안 회전한다. 요청이 오래 걸려도 회전은 2초 후 끝나며 완료될 때 데이터가 갱신된다. [Apple 위젯 상호작용](https://developer.apple.com/documentation/widgetkit/adding-interactivity-to-widgets-and-live-activities), [애니메이션 제약](https://developer.apple.com/documentation/widgetkit/animating-data-updates-in-widgets-and-live-activities)

Xcode 27로 만든 개발 빌드에서 `Failed to fetch metadata for HongsiWidgetAction`이 반복되고, 새로고침 탭이 앱 열기로 처리되는 현상을 재현했다. 메타데이터 조회 문제를 줄이기 위해 `HongsiWidgets` 타깃만 `ENABLE_DEBUG_DYLIB=NO`로 설정해 동작 코드를 확장 실행 파일에 직접 링크한다. 이는 위젯 갤러리 미리보기와 별개이며, 해당 타깃의 Xcode Canvas Previews 실행에는 제약이 생긴다. [Apple 디버그 빌드 구조 안내](https://developer.apple.com/documentation/xcode/understanding-build-product-layout-changes)

## 검증

`pnpm --dir app check`로 공유 Svelte UI를 검사한다. iOS 전용 자동 테스트는 포함하지 않는다.

기능 검증 시 실제 학교 테스트 계정과 연결된 기기에서 로그인·자동 로그인·로그아웃, 계정 전환, 출석 성공/실패, 위치 거절, 과제 파일 업로드·미리보기·공유, 좌석 연장·퇴실, 위젯 링크, 앱 종료 상태, 로컬 알림 및 APNs를 확인해야 한다. Simulator 검사는 실기기의 서명·권한·학교 요청 동작을 대신하지 않는다.
