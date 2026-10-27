# Personal Finance Data

## Contexts
| Name | Color | Notes |
| --- | --- | --- |
| home-repair | #FFA500 | Fixes around the house |
| vacation | #008080 | Hawaii 2026 |

## Categories
| Name | Type | Icon | Notes | Parent | Match Patterns |
| --- | --- | --- | --- | --- | --- |
| Credit Card Payment | transfer | 🏦 | Paying off cards |  |  |
| Dining | expense | 🍔 |  |  | ^star.*;bistro |
| Groceries | expense | 🛒 |  |  | whole foods;kroger |
| Housing | expense | 🏠 | Mortgage and repairs |  | mortgage |
| Income | income | 💵 |  |  | employer;payroll |
| Utilities | expense | ⚡️ |  | Housing | power;water |

## Scenarios
| Name | Color | Notes |
| --- | --- | --- |
| Plan A | #00FF00 | Projected budget |

## Accounts
| Name | Type | Institution | Currency | External |
| --- | --- | --- | --- | --- |
| Brokerage | investment | Fidelity | USD | true |
| Checking | checking | Chase | USD | false |
| Rewards Card | credit | Amex | USD | false |

## Obligations
| Name | Status | Target | Balance | Due Date | Created Date | Updated Date | Notes | Account | Context |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Roof Repair | Completed | 1200 | 1200 |  | 2025-10-01 | 2025-12-01 |  | Checking | home-repair |
| Vacation Fund | Funding | 3000 | 500 | 2026-07-01 | 2026-01-01 | 2026-02-01 | Hawaii trip | Checking | vacation |

## BudgetItems
| Name | Type | Amount | Start Date | End Date | Frequency | Specific Months | Notes | Category | Account |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Groceries | Expense | 150 | 2026-01-05 |  | Weekly |  | Weekly estimate | Groceries | Rewards Card |
| Gym | Bill | 40 | 2026-01-10 | 2026-06-30 | Monthly |  | Cancel in July | Housing | Checking |
| Insurance | Bill | 600 | 2026-02-15 |  | Quarterly |  |  | Housing | Checking |
| Mortgage | Bill | 1500 | 2026-01-31 |  | Monthly |  |  | Housing | Checking |
| Salary | Income | 4000 | 2026-01-25 |  | Monthly |  | Payroll | Income | Checking |
| Water Bill | Bill | 90 | 2026-01-17 |  | Specific Months | 1,3,5,7,9,11 | Bi-monthly | Utilities | Checking |

## ImportMappings
### Monarch (Global)
| Type | Source Value | Target Value | Negate Amounts |
| --- | --- | --- | --- |
| account | chase checking | Checking | false |

### Checking
| Type | Source Value | Target Value | Negate Amounts |
| --- | --- | --- | --- |
| column | Date | Date | false |
| column | Description | Payee | false |
| column | Amount | Amount | false |
| payee | starbucks store 123 | Starbucks | false |

## Transactions
### Brokerage
| Date | Payee | Category | Amount | Memo | Scenario | Contexts | Status | Attachment |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 2026-01-02 | Opening deposit |  | 10000 |  |  |  | cleared |  |

### Checking
| Date | Payee | Category | Amount | Memo | Scenario | Contexts | Status | Attachment |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 2026-04-01 | Mortgage | Housing | -1500 | Auto-generated from Budget | Plan A |  | future |  |
| 2026-03-25 | Employer Payroll | Income | 4000 | Auto-generated from Budget | Plan A |  | future |  |
| 2026-03-10 | Hawaii flights |  | -450 |  |  | vacation | cleared |  |
| 2026-03-05 | Transfer to Rewards Card | Credit Card Payment | -300 |  |  |  | cleared |  |
| 2026-03-01 | City Power | Utilities | -120.45 |  |  |  | pending |  |
| 2026-02-28 | Mortgage | Housing | -1500 |  |  |  | reconciled |  |
| 2026-02-25 | Employer Payroll | Income | 4000 |  |  |  | reconciled |  |
| 2026-02-17 | City Water | Utilities | -88.10 |  |  |  | cleared |  |
| 2026-02-03 | A｜B Hardware | Housing | -64.99 | faucet｜washer |  | home-repair | cleared | /Users/Shared/receipt.pdf |
| 2026-01-31 | Mortgage | Housing | -1500 |  |  |  | reconciled |  |
| 2026-01-25 | Employer Payroll | Income | 4000 |  |  |  | reconciled |  |
| 2026-01-24 | Starting Balance |  | 2500 | Initial account balance |  |  | cleared |  |

### Old Bank
| Date | Payee | Category | Amount | Memo | Scenario | Contexts | Status | Attachment |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 2025-11-01 | Closed account fee |  | -5 |  |  |  | cleared |  |

### Rewards Card
| Date | Payee | Category | Amount | Memo | Scenario | Contexts | Status | Attachment |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 2026-04-05 | Expense: Groceries | Groceries | -150 | Auto-generated from Budget | Plan A |  | future |  |
| 2026-03-12 | Starbucks | Dining | -6.75 |  |  |  | pending |  |
| 2026-03-05 | Transfer from Checking | Credit Card Payment | 300 |  |  |  | cleared |  |
| 2026-03-02 | Whole Foods | Groceries | -142.30 |  |  | vacation,home-repair | cleared |  |
| 2026-02-14 | Bistro Nights | Dining | -88 | Valentine's dinner |  |  | cleared |  |

## Settings
| Key | Value |
| --- | --- |
| lastProjectionScenario | Plan A |
| lastProjectionStartDate | 2026-03-15 |
| lastProjectionEndDate | 2026-12-31 |
