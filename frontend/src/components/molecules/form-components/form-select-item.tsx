import {
  FormField,
  FormItem,
  FormLabel,
  FormControl,
  FormMessage,
} from '@/components/ui/form';
import {
  Select,
  SelectTrigger,
  SelectContent,
  SelectItem,
  SelectValue,
  SelectGroup,
  SelectLabel,
} from '@/components/ui/select';
import { Control } from 'react-hook-form'; // Import Control type
type ISelectItemProps = {
  name: string;
  placeholder: string;
  control: Control<any>;
  label: string;
  formStyle: string;
  formLabelStyle: string;
  formControlStyle: string;
  formMessageStyle: string;
  options: { label: string; value: string }[];
  selectLabel: string;
  defaultValue: string;
};

export const FormSelectItem = ({
  name,
  placeholder,
  control,
  label,
  formStyle,
  formLabelStyle,
  formControlStyle,
  formMessageStyle,
  options,
  selectLabel,
  defaultValue,
}: ISelectItemProps) => {
  return (
    <div>
      <FormField
        control={control} // Use the control prop passed from the parent
        name={name}
        render={({ field }) => (
          <FormItem
          // className={formStyle}
          >
            <div className={formStyle}>
              <FormLabel className={formLabelStyle}>{label}</FormLabel>
              <FormControl className={formControlStyle}>
                <Select
                  onValueChange={field.onChange}
                  value={field.value}
                  defaultValue={defaultValue}
                >
                  <FormControl>
                    <SelectTrigger className="focus:ring-transparent">
                      <SelectValue placeholder={placeholder} />
                    </SelectTrigger>
                  </FormControl>
                  <SelectContent>
                    <SelectGroup>
                      <SelectLabel>{selectLabel}</SelectLabel>
                      {options.map((item, i) => (
                        <SelectItem
                          key={`${item}+${i}`}
                          value={item.value}
                          className="hover:bg-muted cursor-pointer"
                        >
                          {item.label}
                        </SelectItem>
                      ))}
                    </SelectGroup>
                  </SelectContent>
                </Select>
              </FormControl>
            </div>
            <FormMessage className={formMessageStyle} />
          </FormItem>
        )}
      />
    </div>
  );
};
