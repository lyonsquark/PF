# Personal Finance Data

## Contexts
| Name | Color | Notes |
| --- | --- | --- |
| Vacation | #FFD700 | Hawaii Trip |
| Home Renovation | #8B4513 | Kitchen remodel |
| Tax Prep 2026 | #FF4500 | End of year taxes |

## Categories
| Name | Type | Icon | Notes | Parent |
| --- | --- | --- | --- | --- |
| Salary | income | dollarsign.circle | Primary Income | |
| Bonus | income | dollarsign.circle.fill | End of year bonus | Salary |
| Housing | expense | house | Rent/Mortgage | |
| Groceries | expense | cart | Food shopping | |
| Dining Out | expense | fork.knife | Restaurants | |
| Utilities | expense | bolt | Power and Water | Housing |
| Transportation | expense | car | Gas and Transit | |
| Entertainment | expense | tv | Movies and Games | |

## Scenarios
| Name | Color | Notes |
| --- | --- | --- |
| Aggressive Savings | #00FF00 | Cutting back dining out |
| Vacation Prep | #FFA500 | Saving extra for Hawaii |

## Accounts
| Name | Type | Institution | Currency | External |
| --- | --- | --- | --- | --- |
| Checking | checking | Chase | USD | false |
| Simple Savings | savings | Ally | USD | false |
| Sapphire Credit | credit | Chase | USD | false |
| Mortgage | loan | Rocket | USD | false |

## Obligations
| Name | Status | Target | Balance | Due Date | Created Date | Updated Date | Notes | Account | Context |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Down Payment | Funding | 50000 | 12500 | 2026-12-01T00:00:00Z | 2026-01-01T00:00:00Z | 2026-02-01T00:00:00Z | New house fund | Simple Savings | |
| Property Tax | Funding | 6000 | 3000 | 2026-10-15T00:00:00Z | 2026-01-01T00:00:00Z | 2026-02-05T00:00:00Z | Annual tax requirement | Checking | Tax Prep 2026 |
| Kitchen Faucet | Spending | 250 | 0 | 2026-03-01T00:00:00Z | 2026-02-10T00:00:00Z | 2026-02-15T00:00:00Z | Fix the leak | Checking | Home Renovation |

## BudgetItems
| Name | Type | Amount | Start Date | Frequency | Specific Months | Notes | Category | Account |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Tech Salary | income | 5000 | 2026-01-01T00:00:00Z | monthly | | Direct Deposit | Salary | Checking |
| Tech Salary | income | 5000 | 2026-01-15T00:00:00Z | monthly | | Direct Deposit | Salary | Checking |
| Apartment Rent | expense | 2200 | 2026-01-01T00:00:00Z | monthly | | Paid on the 1st | Housing | Checking |
| Electric Bill | expense | 120 | 2026-01-15T00:00:00Z | monthly | | Varies | Utilities | Checking |
| Water Bill | expense | 50 | 2026-01-20T00:00:00Z | quarterly | | Varies | Utilities | Checking |
| Groceries | expense | 450 | 2026-01-01T00:00:00Z | monthly | | Target $450/month | Groceries | Sapphire Credit |
| Dining Out | expense | 300 | 2026-01-01T00:00:00Z | monthly | | Weekend treats | Dining Out | Sapphire Credit |
| Annual Bonus | income | 8000 | 2026-12-15T00:00:00Z | yearly | | Holiday bonus | Bonus | Simple Savings |

## Transactions
| Date | Amount | Payee | Memo | Account | Category | Contexts | Status | Scenario |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 2025-12-01T12:00:00Z | -2200 | Avalon Communities | Rent | Checking | Housing | | cleared | |
| 2025-12-05T12:00:00Z | -150.25 | Whole Foods | Weekly groceries | Sapphire Credit | Groceries | | cleared | |
| 2025-12-10T12:00:00Z | -25.50 | Chipotle | Quick lunch | Sapphire Credit | Dining Out | | cleared | |
| 2025-12-12T12:00:00Z | -110.00 | Electric Co | | Checking | Utilities | | cleared | |
| 2025-12-15T12:00:00Z | 5000 | Acme Corp | Direct Deposit | Checking | Salary | | cleared | |
| 2025-12-20T12:00:00Z | -200 | Target | Gifts | Sapphire Credit | Entertainment | | cleared | |
| 2025-12-25T12:00:00Z | -85.00 | AMC Theaters | Star Wars | Sapphire Credit | Entertainment | | cleared | |
| 2026-01-01T12:00:00Z | -2200 | Avalon Communities | Rent | Checking | Housing | | cleared | |
| 2026-01-02T12:00:00Z | -140 | Trader Joes | | Sapphire Credit | Groceries | | cleared | |
| 2026-01-08T12:00:00Z | -45.00 | Local Diner | Breakfast | Sapphire Credit | Dining Out | | cleared | |
| 2026-01-14T12:00:00Z | -125.00 | Electric Co | | Checking | Utilities | | cleared | |
| 2026-01-15T12:00:00Z | 5000 | Acme Corp | Direct Deposit | Checking | Salary | | cleared | |
| 2026-01-18T12:00:00Z | -150 | Home Depot | Faucet parts | Sapphire Credit | Housing | Home Renovation | cleared | |
| 2026-01-20T12:00:00Z | -50.00 | City Water | | Checking | Utilities | | cleared | |
| 2026-01-22T12:00:00Z | -35.00 | Uber | Ride to airport | Sapphire Credit | Transportation | Vacation | cleared | |
| 2026-01-22T12:00:00Z | -450.00 | Delta Airlines | Flights | Sapphire Credit | Entertainment | Vacation | cleared | |
| 2026-01-25T12:00:00Z | -200 | Trader Joes | Party prep | Sapphire Credit | Groceries | | cleared | |
| 2026-02-01T12:00:00Z | -2200 | Avalon Communities | Rent | Checking | Housing | | cleared | |
| 2026-02-05T12:00:00Z | -160.00 | Whole Foods | | Sapphire Credit | Groceries | | cleared | |
| 2026-02-12T12:00:00Z | -60.00 | Gas Station | Fill up | Sapphire Credit | Transportation | | cleared | |
| 2026-02-14T12:00:00Z | -150.00 | Fancy Restaurant | Valentines day | Sapphire Credit | Dining Out | | cleared | |
| 2026-02-15T12:00:00Z | 5000 | Acme Corp | Direct Deposit | Checking | Salary | | cleared | |
| 2026-02-20T12:00:00Z | -85.00 | Electric Co | | Checking | Utilities | | pending | |
