# Personal Finance Data

## Contexts
| Name | Color | Notes |
| --- | --- | --- |
| Taxes | #FF4500 | Property tax |
| Vacation | #FFD700 | Hawaii |

## Categories
| Name | Type | Icon | Notes | Parent | Match Patterns |
| --- | --- | --- | --- | --- | --- |
| Dining | expense | 🍽️ |  |  | Starbucks;Cafe |
| Groceries | expense |  |  |  | Whole Foods |
| Rent | expense |  |  |  |  |
| Salary | income |  |  |  | Acme Payroll |
| Transfer | transfer |  |  |  |  |
| Utilities | expense |  |  | Rent |  |

## Scenarios
| Name | Color | Notes |
| --- | --- | --- |
| Plan A | #00AA00 | Test plan |

## Accounts
| Name | Type | Institution | Currency | External |
| --- | --- | --- | --- | --- |
| Checking | checking | Chase | USD | false |
| External 401k | investment | Fidelity | USD | true |
| Mortgage | loan | Rocket | USD | false |
| Savings | savings | Ally | USD | false |
| Visa | credit | Chase | USD | false |

## Obligations
| Name | Status | Target | Balance | Due Date | Created Date | Updated Date | Notes | Account | Context |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Property Tax | Funding | 2400 | 1000 | 2026-10-15T04:00:00Z | 2026-01-01T05:00:00Z | 2026-01-01T05:00:00Z |  | Checking | Taxes |
| Vacation Fund | Completed | 2000 | 2000 |  | 2026-01-01T05:00:00Z | 2026-01-01T05:00:00Z |  | Savings | Vacation |

## BudgetItems
| Name | Type | Amount | Start Date | End Date | Frequency | Specific Months | Notes | Category | Account |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Acme Payroll | Income | 4000 | 2026-01-15T05:00:00Z |  | Monthly |  |  | Salary | Checking |
| Car Insurance | Bill | 450 | 2026-02-15T05:00:00Z |  | Quarterly |  |  |  | Checking |
| Groceries | Expense | 600 | 2026-01-31T05:00:00Z |  | Monthly |  |  | Groceries | Checking |
| Landlord | Bill | 1500 | 2026-01-01T05:00:00Z |  | Monthly |  |  | Rent | Checking |
| Property Tax | Bill | 1200 | 2026-04-30T04:00:00Z |  | Specific Months | 4,10 |  |  | Checking |

## ImportMappings

## Transactions
### Checking
| Date | Payee | Category | Amount | Memo | Scenario | Contexts | Status | Attachment |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 2026-04-01T04:00:00Z | Landlord | Rent | -1500 | Auto-generated from Budget | Plan A |  | future |  |
| 2026-03-12T04:00:00Z | Transfer to Savings | Transfer | -500 |  |  |  | cleared |  |
| 2026-03-10T04:00:00Z | Whole Foods | Groceries | -142.3 |  |  |  | pending |  |
| 2026-02-10T05:00:00Z | County Tax Office |  | -300 |  |  | Taxes | cleared |  |
| 2026-02-01T05:00:00Z | Landlord | Rent | -1500 |  |  |  | cleared |  |
| 2026-01-20T05:00:00Z | Starbucks | Dining | -5.75 |  |  |  | cleared |  |
| 2026-01-15T05:00:00Z | Acme Payroll | Salary | 4000 |  |  |  | cleared |  |
| 2026-01-02T05:00:00Z | Landlord | Rent | -1500 |  |  |  | reconciled |  |
| 2025-12-31T05:00:00Z | Starting Balance |  | 2000 | Initial account balance |  |  | cleared |  |

### External 401k
| Date | Payee | Category | Amount | Memo | Scenario | Contexts | Status | Attachment |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 2026-01-31T05:00:00Z | Contribution |  | 1000 |  |  |  | cleared |  |

### Savings
| Date | Payee | Category | Amount | Memo | Scenario | Contexts | Status | Attachment |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 2026-03-12T04:00:00Z | Transfer from Checking | Transfer | 500 |  |  |  | cleared |  |
| 2026-01-01T05:00:00Z | Starting Balance |  | 10000 | Initial account balance |  |  | cleared |  |

### Visa
| Date | Payee | Category | Amount | Memo | Scenario | Contexts | Status | Attachment |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 2026-02-14T05:00:00Z | Hotel Maui |  | -650 |  |  | Vacation | cleared |  |
| 2026-01-25T05:00:00Z | Cafe Luna |  | -18.4 |  |  |  | cleared |  |

