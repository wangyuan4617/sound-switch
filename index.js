const VENDOR_ID = 1008
const PRODUCT_ID = 2702
let hypex=[]

// 设备开机
const power_on=Buffer.from([0x64, 0x01]);
// 设备关机
const power_off=Buffer.from([0x64, 0x03]);




// 音量加大，操作音量加大会收到一个volume_up+一个command_end
const volume_up =Buffer.from([0x01, 0x01,0x00,0x00,0x00]);
// 音量减小，操作音量减小会收到一个volume_down+一个command_end
const volume_down =Buffer.from([0x01, 0x02,0x00,0x00,0x00]);
// 指令结束
const command_end =Buffer.from([0x01, 0x00,0x00,0x00,0x00]);

// 麦克风开启
const mike_on=Buffer.from([0x65,0x00])
// 麦克风静音
const mike_off=Buffer.from([0x65,0x04])

// 不明指令
const command_0=Buffer.from([0x01, 0x7f, 0x00, 0x00, 0x00,])

const main=async ()=>{
    const HID = await import('node-hid');
    const devices = await HID.devicesAsync();
    const ers=devices.filter(item=>item.productId===PRODUCT_ID && item.vendorId===VENDOR_ID)
    console.log('找到的耳机设备')
    console.log(ers)
    if(ers && ers.length!==0){
        try{
        hypex=ers.map(er=>{
            return {
                instance:new HID.HID(er.path),
                path:er.path
            }
        })
        }catch (e) {
            console.error(e)
        }
    }
    if(!hypex.length){
        console.error('no device')
        return
    }
    console.log('绑定监听数据---')
    hypex.forEach(item=>{
        item.instance.on('data',(data)=>{
            console.log('path'+item.path)
            console.log(data)
            if(power_on.equals(data)){
                console.log('匹配到了开机',data)
            }
            if(power_off.equals(data)){
                console.log('匹配到了关机',data)
            }
            if(volume_up.equals(data)){
                console.log('匹配到了音量加',data)
            }
            if(volume_down.equals(data)){
                console.log('匹配到了音量减',data)
            }
            if(command_end.equals(data)){
                console.log('匹配到了指令结束',data)
            }
            if(command_0.equals(data)){
                console.log('匹配到了不明指令0',data)
            }
            if(mike_on.equals(data)){
                console.log('匹配到了麦克风开启',data)
            }
            if(mike_off.equals(data)){
                console.log('匹配到了麦克风静音',data)
            }
        })
        item.instance.on('error',(e)=>{
            console.log('出错了: '+item.path)
            console.log(e)
        })

    })

}
main()