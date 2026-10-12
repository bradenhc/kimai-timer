# Round only when reporting

Recorded times are stored exactly (to the second) and never rounded. Rounding is
applied only in a Report, to each Project's total time on a Day, and always
rounds up to the next step of the Rounding Mode. Rounding individual Intervals
would compound the error across a day and lose information that can never be
recovered; rounding the daily total keeps the error to at most one step per
Project per Day. The default mode is Decimal (steps of 0.01 hours, or 36
seconds), so that reported hours times an hourly rate never produces a repeating
decimal on an invoice. Classic mode (steps of n minutes, typically 3) exists to
match Kimai's own rounding conventions.
