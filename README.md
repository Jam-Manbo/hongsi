<div align="center">

<img src="app/public/favicon.svg" width="96" height="96" alt="홍시" />

# 홍시

### 홍대생의 시간을 효율적으로
> 출결부터 과제 마감, 열람실 현황과 학식 확인까지 한곳에서.

![Rust](https://img.shields.io/badge/Rust-2021-000000?logo=rust&logoColor=white)
![Tauri 2](https://img.shields.io/badge/Tauri-2-24C8D8?logo=tauri&logoColor=white)
![Svelte 5](https://img.shields.io/badge/Svelte-5-FF3E00?logo=svelte&logoColor=white)
![Axum](https://img.shields.io/badge/Axum-0.8-6E4A7E)
![PostgreSQL](https://img.shields.io/badge/PostgreSQL-18-4169E1?logo=postgresql&logoColor=white)

[**사용하기**](#홍시-사용하기) | [**데모 체험하기**](https://hongsi-demo.vercel.app)

</div>

---

## 홍시 사용하기

| 링크 | 내용 |
| --- | --- |
| [웹에서 사용](https://hongsi.kyuyoung.dev/) | 브라우저에서 바로 이용 |
| [앱 다운로드](https://hongsi.kyuyoung.dev/download) | Android용 APK 다운로드 |
| [iOS 이용 안내](https://hongsi.kyuyoung.dev/download/ios) | iOS 임시 설치 안내 |

-  앱스토어와 플레이스토어 등록은 준비중이에요.
- [등록비 후원](https://ko-fi.com/manbo)으로 앱 배포 준비를 도울 수 있어요.

## 주요 기능

| 영역 | 주요 기능 |
| --- | --- |
| **전자출결** | 빠른 출결, 과목별 출결 기록, 주간 시간표 확인 |
| **캘린더** | 과제, 강의 등의 마감과 제출 여부, 시청 상태를 확인 |
| **과제 제출** | 앱과 웹에서 파일 제출, 제출한 파일 수정 |
| **클래스룸 알림** | 클래스룸 공지사항과 첨부파일을 확인 |
| **열람실** | 열람실 현황 확인, 남은 시간과 종료 알림 관리 |
| **학식** | 학생 식당의 메뉴와 가격 확인 |
| **할 일** | 캘린더에서 할 일 및 마감 알림을 관리 |
| **학생증 QR** | 학생증 QR 빠른 확인 |

## 문서

| 문서 | 내용 |
| --- | --- |
| [기본 사용법](docs/user-guide.md) | 기본 사용법 안내 |
| [출결](docs/features/attendance.md) | 출석번호 입력, 출결 상태 및 주간 시간표 |
| [캘린더](docs/features/calendar.md) | 일정 확인, 완료 체크, 과제 제출 및 마감 알림 |
| [열람실](docs/features/seats.md) | 현황 조회, 이용 기록 및 종료 알림 |
| [학식](docs/features/meals.md) | 일별 메뉴 확인 |

## 알아두기

- <u>**서비스 이용에 관한 책임은 사용자에게 있어요.**</u>

    서비스 이용으로 발생할 수 있는 불이익에 대해서는 책임을 지지 않아요. 홍시는 개인이 관리하는 비공식적인 프로젝트이며 언제든 기능이 작동하지 않거나 지원이 중단될 수 있어요. 홍익대학교 및 신한은행이 제공하지 않으며 보증하지 않아요.

- **중요한 기록은 학교 시스템에서 확인해 주세요.**

    출결, 과제 제출 등 중요한 기록은 직접 확인해주세요. 홍시에서 완료로 체크해도 실제 과제가 제출되거나 강의가 이수되는 것은 아니며, 작동하지 않을 가능성이 있어요.

- **알림은 늦거나 오지 않을 수 있어요.**

    네트워크 상태, 기기 권한과 절전 설정의 영향을 받아요. 출석과 과제 마감은 알림에만 의존하지 말고 직접 확인해 주세요.

- **열람실 알림은 실제 좌석을 배치해주지 않아요.**

    홍시에서는 배정받은 좌석의 이용 시간과 알림만 관리해요. 실제 좌석 배정, 연장과 퇴실은 학교의 좌석 배정기에서만 가능해요.

- **기기 간 동기화에는 서버에 저장한 정보를 사용해요.**

    할 일, 완료 체크, 좌석 이용 기록을 서버에 저장해 다른 기기에서도 이어서 사용할 수 있어요. 백그라운드 동기화에는 암호화한 학교 인증정보를 사용해요. 비밀번호는 저장하지 않아요.


---

<div align="center">

Copyright 2026 홍시 / [GNU AGPLv3](./LICENSE)

</div>
